use nervusdb_v2_api::InternalNodeId;
use nervusdb_v2_query::Value;
use pyo3::prelude::*;
use std::collections::BTreeMap;

#[pyclass]
#[derive(Debug, Clone)]
pub struct Node {
    #[pyo3(get)]
    pub id: u64,
    #[pyo3(get)]
    pub labels: Vec<String>,
    #[pyo3(get)]
    pub properties: BTreeMap<String, PyObject>,
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct Relationship {
    #[pyo3(get)]
    pub id: Option<u64>,
    #[pyo3(get)]
    pub start_node_id: u64,
    #[pyo3(get)]
    pub end_node_id: u64,
    #[pyo3(get)]
    pub rel_type: String,
    #[pyo3(get)]
    pub properties: BTreeMap<String, PyObject>,
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct Path {
    #[pyo3(get)]
    pub nodes: Vec<Node>,
    #[pyo3(get)]
    pub relationships: Vec<Relationship>,
}

/// Convert a generic Python object to a NervusDB Value.
pub fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }

    if let Ok(b) = obj.extract::<bool>() {
        return Ok(Value::Bool(b));
    }

    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Value::Int(i));
    }

    if let Ok(f) = obj.extract::<f64>() {
        return Ok(Value::Float(f));
    }

    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::String(s));
    }

    if let Ok(list) = obj.downcast::<pyo3::types::PyList>() {
        let mut vec = Vec::new();
        for item in list {
            vec.push(py_to_value(&item)?);
        }
        return Ok(Value::List(vec));
    }

    // Note: PyDict check should handle string keys
    if let Ok(dict) = obj.downcast::<pyo3::types::PyDict>() {
        let mut map = std::collections::BTreeMap::new();
        for (k, v) in dict {
            let key = k.extract::<String>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("Dictionary keys must be strings")
            })?;
            map.insert(key, py_to_value(&v)?);
        }
        return Ok(Value::Map(map));
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "Unsupported type for PropertyValue",
    ))
}

/// Convert a NervusDB Value to a Python object.
pub fn value_to_py(val: Value, py: Python<'_>) -> Py<PyAny> {
    match val {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Int(i) => i.into_py(py),
        Value::Float(f) => f.into_py(py),
        Value::String(s) => s.into_py(py),
        Value::List(list) => {
            let py_list =
                pyo3::types::PyList::new_bound(py, list.into_iter().map(|v| value_to_py(v, py)));
            py_list.into()
        }
        Value::Map(map) => {
            let py_dict = pyo3::types::PyDict::new_bound(py);
            for (k, v) in map {
                let _ = py_dict.set_item(k, value_to_py(v, py));
            }
            py_dict.into()
        }
        Value::Node(n) => {
            let mut props = BTreeMap::new();
            for (k, v) in n.properties {
                props.insert(k, value_to_py(v, py));
            }
            Node {
                id: n.id.0,
                labels: n.labels,
                properties: props,
            }
            .into_py(py)
        }
        Value::Relationship(r) => {
            let mut props = BTreeMap::new();
            for (k, v) in r.properties {
                props.insert(k, value_to_py(v, py));
            }
            Relationship {
                id: r.id.map(|k| k.src ^ k.dst ^ 0x0102030405060708), // Dummy stable ID for now
                start_node_id: r.start.0,
                end_node_id: r.end.0,
                rel_type: r.rel_type,
                properties: props,
            }
            .into_py(py)
        }
        Value::ReifiedPath(p) => {
            let nodes = p
                .nodes
                .into_iter()
                .map(|n| {
                    let mut props = BTreeMap::new();
                    for (k, v) in n.properties {
                        props.insert(k, value_to_py(v, py));
                    }
                    Node {
                        id: n.id.0,
                        labels: n.labels,
                        properties: props,
                    }
                })
                .collect();
            let rels = p
                .relationships
                .into_iter()
                .map(|r| {
                    let mut props = BTreeMap::new();
                    for (k, v) in r.properties {
                        props.insert(k, value_to_py(v, py));
                    }
                    Relationship {
                        id: r.id.map(|k| k.src ^ k.dst ^ 0x0102030405060708),
                        start_node_id: r.start.0,
                        end_node_id: r.end.0,
                        rel_type: r.rel_type,
                        properties: props,
                    }
                })
                .collect();
            Path {
                nodes,
                relationships: rels,
            }
            .into_py(py)
        }
        Value::NodeId(id) => id.0.into_py(py),
        Value::EdgeKey(key) => format!("{key:?}").into_py(py),
        Value::Path(p) => {
            // Deprecated Path representation, but let's keep it for compatibility if needed
            let mut out = BTreeMap::new();
            out.insert(
                "nodes".to_string(),
                p.nodes
                    .iter()
                    .map(|id| id.0)
                    .collect::<Vec<_>>()
                    .into_py(py),
            );
            out.insert(
                "edges".to_string(),
                p.edges
                    .iter()
                    .map(|k| format!("{k:?}"))
                    .collect::<Vec<_>>()
                    .into_py(py),
            );
            out.into_py(py)
        }
        _ => py.None(),
    }
}

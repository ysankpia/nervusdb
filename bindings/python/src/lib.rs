#![allow(unexpected_cfgs)]
// pyo3 的 `#[pymethods]` 宏会在每个以 `PyResult<T>` 为返回类型的导出方法处注入
// `.into()`，当 T 本身已是 `PyErr`/`PyObject` 系列时即触发 `useless_conversion`。
// 这是宏生成代码而非手写逻辑，无法在源码层面消除，故在此 crate 内定点豁免。
#![allow(clippy::useless_conversion)]

use graphlite_core::{
    GraphError, GraphLite as CoreGraphLite, Transaction as CoreTransaction, Value,
};
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

/// 解析方向参数："out"/"outgoing"、"in"/"incoming"，缺省为双向
fn parse_direction(direction: Option<&str>) -> PyResult<graphlite_core::Direction> {
    match direction.map(|d| d.to_ascii_lowercase()) {
        None => Ok(graphlite_core::Direction::Both),
        Some(d) => match d.as_str() {
            "out" | "outgoing" => Ok(graphlite_core::Direction::Outgoing),
            "in" | "incoming" => Ok(graphlite_core::Direction::Incoming),
            "both" | "any" => Ok(graphlite_core::Direction::Both),
            other => Err(GraphLiteError::new_err(format!(
                "invalid direction '{}': expected outgoing/incoming/both",
                other
            ))),
        },
    }
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

    #[pyo3(signature = (start, end, edge_type = None))]
    pub fn bfs(&self, start: u64, end: u64, edge_type: Option<&str>) -> PyResult<Option<Vec<u64>>> {
        Ok(self.inner.bfs(start, end, edge_type))
    }

    pub fn has_cycle(&self) -> PyResult<bool> {
        Ok(self.inner.has_cycle())
    }

    /// PageRank 阻尼迭代，返回 [(node_id, score), ...]（按分数降序）
    #[pyo3(signature = (damping_factor = 0.85, max_iterations = 100, tolerance = 1e-6))]
    pub fn pagerank(
        &self,
        py: Python<'_>,
        damping_factor: f64,
        max_iterations: usize,
        tolerance: f64,
    ) -> PyResult<PyObject> {
        let list = PyList::empty_bound(py);
        for score in self
            .inner
            .pagerank_with(damping_factor, max_iterations, tolerance)
        {
            let tup = (score.node_id, score.score);
            list.append(tup)?;
        }
        Ok(list.into_py(py))
    }

    /// 弱连通分量分析，返回 [[node_id, ...], ...]（按分量规模降序）
    pub fn weakly_connected_components(&self, py: Python<'_>) -> PyResult<PyObject> {
        let list = PyList::empty_bound(py);
        for component in self.inner.weakly_connected_components() {
            list.append(component)?;
        }
        Ok(list.into_py(py))
    }

    /// K-Hop 局部子图提取，返回 {"nodes": [...], "edges": [...]}
    #[pyo3(signature = (start, k, direction = None, edge_type = None))]
    pub fn k_hop_subgraph(
        &self,
        py: Python<'_>,
        start: u64,
        k: usize,
        direction: Option<&str>,
        edge_type: Option<&str>,
    ) -> PyResult<PyObject> {
        let dir = parse_direction(direction)?;
        let sub = self
            .inner
            .k_hop_subgraph_with(start, k, dir, edge_type)
            .map_err(to_py_err)?;

        let dict = PyDict::new_bound(py);
        dict.set_item("nodes", sub.nodes)?;

        let edges = PyList::empty_bound(py);
        for edge in sub.edges {
            let e = PyDict::new_bound(py);
            e.set_item("id", edge.id)?;
            e.set_item("src_id", edge.src_id)?;
            e.set_item("dst_id", edge.dst_id)?;
            e.set_item("edge_type", edge.edge_type)?;
            e.set_item("weight", edge.weight)?;
            let props = PyDict::new_bound(py);
            for (k, v) in &edge.properties {
                props.set_item(k, value_to_py(py, v)?)?;
            }
            e.set_item("properties", props)?;
            edges.append(e)?;
        }
        dict.set_item("edges", edges)?;
        Ok(dict.into_py(py))
    }

    /// 图模式：全部标签与关系类型
    pub fn labels(&self) -> PyResult<Vec<String>> {
        Ok(self.inner.labels())
    }

    pub fn edge_types(&self) -> PyResult<Vec<String>> {
        Ok(self.inner.edge_types())
    }

    /// 导出当前图数据为可回灌的 Cypher 脚本并返回字符串
    pub fn dump_cypher(&self) -> PyResult<String> {
        let mut buffer: Vec<u8> = Vec::new();
        self.inner.dump_cypher(&mut buffer).map_err(to_py_err)?;
        String::from_utf8(buffer)
            .map_err(|e| GraphLiteError::new_err(format!("dump is not valid UTF-8: {}", e)))
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
        dict.set_item("wal_page_count", stats.wal_page_count)?;
        dict.set_item("spill_count", stats.spill_count)?;
        dict.set_item("wal_size_bytes", stats.wal_size_bytes)?;
        dict.set_item("wal_fsync_count", stats.wal_fsync_count)?;
        dict.set_item("wal_frames_written", stats.wal_frames_written)?;
        Ok(dict.into_py(py))
    }

    pub fn checkpoint(&self) -> PyResult<()> {
        self.inner.checkpoint().map_err(to_py_err)?;
        Ok(())
    }

    /// 开启显式事务（推荐配合 `with db.begin() as tx:` 使用）
    pub fn begin_transaction(&self) -> PyResult<PyTransaction> {
        let tx = self.inner.begin_transaction().map_err(to_py_err)?;
        Ok(PyTransaction { inner: Some(tx) })
    }
}

/// 显式批量事务：脏页在 `commit()` 时批量写入 WAL 并**只做一次 fsync**。
///
/// 支持上下文管理器语义，异常时自动回滚：
/// ```python
/// with db.begin_transaction() as tx:
///     for i in range(100_000):
///         tx.add_node(["Bulk"], {"idx": i})
/// ```
#[pyclass(name = "Transaction")]
pub struct PyTransaction {
    inner: Option<CoreTransaction>,
}

#[pymethods]
impl PyTransaction {
    pub fn tx_id(&self) -> PyResult<u64> {
        self.inner
            .as_ref()
            .map(|tx| tx.tx_id())
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))
    }

    #[pyo3(signature = (labels, properties = None))]
    pub fn add_node(
        &mut self,
        labels: Vec<String>,
        properties: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<u64> {
        let labels_set: HashSet<String> = labels.into_iter().collect();
        let props = extract_properties(properties)?;
        self.inner
            .as_mut()
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))?
            .add_node(labels_set, props)
            .map_err(to_py_err)
    }

    #[pyo3(signature = (src, dst, edge_type, properties = None, weight = 1.0))]
    pub fn add_edge(
        &mut self,
        src: u64,
        dst: u64,
        edge_type: &str,
        properties: Option<&Bound<'_, PyDict>>,
        weight: f64,
    ) -> PyResult<u64> {
        let props = extract_properties(properties)?;
        self.inner
            .as_mut()
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))?
            .add_edge(src, dst, edge_type, props, weight)
            .map_err(to_py_err)
    }

    #[pyo3(signature = (node_id, key, value))]
    pub fn update_node_property(
        &mut self,
        node_id: u64,
        key: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let val = pyany_to_value(value)?;
        self.inner
            .as_mut()
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))?
            .update_node_property(node_id, key, val);
        Ok(())
    }

    #[pyo3(signature = (edge_id, key, value))]
    pub fn update_edge_property(
        &mut self,
        edge_id: u64,
        key: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let val = pyany_to_value(value)?;
        self.inner
            .as_mut()
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))?
            .update_edge_property(edge_id, key, val);
        Ok(())
    }

    pub fn remove_node(&mut self, node_id: u64) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))?
            .remove_node(node_id);
        Ok(())
    }

    pub fn remove_edge(&mut self, edge_id: u64) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| GraphLiteError::new_err("transaction already finished"))?
            .remove_edge(edge_id);
        Ok(())
    }

    /// 提交事务：所有变更原子应用，仅触发一次 WAL fsync
    pub fn commit(&mut self) -> PyResult<()> {
        let tx = self.inner.take().ok_or_else(|| {
            GraphLiteError::new_err("transaction already committed or rolled back")
        })?;
        tx.commit().map_err(to_py_err)
    }

    /// 回滚事务：丢弃全部未提交变更，主库零污染
    pub fn rollback(&mut self) -> PyResult<()> {
        let tx = self.inner.take().ok_or_else(|| {
            GraphLiteError::new_err("transaction already committed or rolled back")
        })?;
        tx.rollback().map_err(to_py_err)
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (exc_type = None, exc_value = None, traceback = None))]
    fn __exit__(
        &mut self,
        exc_type: Option<Bound<'_, PyAny>>,
        exc_value: Option<Bound<'_, PyAny>>,
        traceback: Option<Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        let _ = (exc_value, traceback);
        if let Some(tx) = self.inner.take() {
            // 上下文内出现异常则回滚，否则提交
            if exc_type.is_some() {
                tx.rollback().map_err(to_py_err)?;
            } else {
                tx.commit().map_err(to_py_err)?;
            }
        }
        Ok(false)
    }
}

#[pymodule]
fn graphlite(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGraphLite>()?;
    m.add_class::<PyTransaction>()?;
    m.add("GraphLiteError", m.py().get_type_bound::<GraphLiteError>())?;
    Ok(())
}

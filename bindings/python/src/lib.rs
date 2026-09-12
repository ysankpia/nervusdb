#![allow(unexpected_cfgs)]
// pyo3 的 `#[pymethods]` 宏会在每个以 `PyResult<T>` 为返回类型的导出方法处注入
// `.into()`，当 T 本身已是 `PyErr`/`PyObject` 系列时即触发 `useless_conversion`。
// 这是宏生成代码而非手写逻辑，无法在源码层面消除，故在此 crate 内定点豁免。
#![allow(clippy::useless_conversion)]

use nervusdb_core::{GraphError, NervusDb as CoreNervusDb, Transaction as CoreTransaction, Value};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::{HashMap, HashSet};

create_exception!(nervusdb, NervusDbError, PyException);

fn to_py_err(err: GraphError) -> PyErr {
    NervusDbError::new_err(err.to_string())
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
        // Python 侧用 None 表示 null，与 `python_to_value` 的映射对称
        Value::Null => Ok(py.None()),
        Value::Int(i) => Ok(i.into_py(py)),
        Value::Float(f) => Ok(f.into_py(py)),
        Value::String(s) => Ok(s.clone().into_py(py)),
        Value::Bool(b) => Ok(b.into_py(py)),
        Value::List(items) => {
            // 递归映射为 Python list，顺序保持一致
            let list = PyList::empty_bound(py);
            for item in items {
                list.append(value_to_py(py, item)?)?;
            }
            Ok(list.into_py(py))
        }
    }
}

/// 批量节点的单项：`(labels, properties)`。
///
/// 用具名别名而非裸元组：这个类型在解析、批量接口与文档里各出现一次，
/// 匿名元组会让三处签名难以对照（clippy 的 `type_complexity` 也在提示这一点）。
type NodeItem = (HashSet<String>, HashMap<String, Value>);

/// 批量边的单项：`(src, dst, edge_type, properties, weight)`。
type EdgeItem = (u64, u64, String, HashMap<String, Value>, f64);

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

/// 把一个 `(labels, properties)` 元组解析为批量节点项。
fn parse_node_item(item: &Bound<'_, PyAny>) -> PyResult<(HashSet<String>, HashMap<String, Value>)> {
    let labels: Vec<String> = item.get_item(0)?.extract()?;
    let labels_set: HashSet<String> = labels.into_iter().collect();

    // 属性可选：省略时视作空字典，与逐条 `add_node(labels)` 的默认行为一致
    let props = match item.get_item(1) {
        Ok(p) if !p.is_none() => {
            let d = p
                .downcast::<PyDict>()
                .map_err(|_| NervusDbError::new_err("node properties must be a dict or None"))?;
            extract_properties(Some(d))?
        }
        _ => HashMap::new(),
    };
    Ok((labels_set, props))
}

/// 解析节点批量参数。接受任意可迭代对象，每项为 `(labels, properties)`。
///
/// 错误信息带上**元素下标**：批量输入里出现类型错误时，"第几个"是最重要的定位
/// 信息，没有它用户只能自己二分查找。
fn parse_node_batch(nodes: &Bound<'_, PyAny>) -> PyResult<Vec<NodeItem>> {
    let mut out = Vec::new();
    for (idx, item) in nodes.iter()?.enumerate() {
        let item = item?;
        out.push(parse_node_item(&item).map_err(|e| {
            NervusDbError::new_err(format!("nodes[{}]: {}", idx, e.value_bound(item.py())))
        })?);
    }
    Ok(out)
}

/// 把一个 `(src, dst, edge_type[, properties[, weight]])` 元组解析为批量边项。
fn parse_edge_item(item: &Bound<'_, PyAny>) -> PyResult<EdgeItem> {
    let src: u64 = item.get_item(0)?.extract()?;
    let dst: u64 = item.get_item(1)?.extract()?;
    let ty: String = item.get_item(2)?.extract()?;

    // 属性与权重可省略，默认与逐条 `add_edge(src, dst, ty)` 一致
    let props = match item.get_item(3) {
        Ok(p) if !p.is_none() => {
            let d = p
                .downcast::<PyDict>()
                .map_err(|_| NervusDbError::new_err("edge properties must be a dict or None"))?;
            extract_properties(Some(d))?
        }
        _ => HashMap::new(),
    };
    let weight = match item.get_item(4) {
        Ok(w) if !w.is_none() => w.extract::<f64>()?,
        _ => 1.0,
    };
    Ok((src, dst, ty, props, weight))
}

/// 解析边批量参数。接受任意可迭代对象，每项为
/// `(src, dst, edge_type[, properties[, weight]])`。
fn parse_edge_batch(edges: &Bound<'_, PyAny>) -> PyResult<Vec<EdgeItem>> {
    let mut out = Vec::new();
    for (idx, item) in edges.iter()?.enumerate() {
        let item = item?;
        out.push(parse_edge_item(&item).map_err(|e| {
            NervusDbError::new_err(format!("edges[{}]: {}", idx, e.value_bound(item.py())))
        })?);
    }
    Ok(out)
}

/// 解析方向参数："out"/"outgoing"、"in"/"incoming"，缺省为双向
fn parse_direction(direction: Option<&str>) -> PyResult<nervusdb_core::Direction> {
    match direction.map(|d| d.to_ascii_lowercase()) {
        None => Ok(nervusdb_core::Direction::Both),
        Some(d) => match d.as_str() {
            "out" | "outgoing" => Ok(nervusdb_core::Direction::Outgoing),
            "in" | "incoming" => Ok(nervusdb_core::Direction::Incoming),
            "both" | "any" => Ok(nervusdb_core::Direction::Both),
            other => Err(NervusDbError::new_err(format!(
                "invalid direction '{}': expected outgoing/incoming/both",
                other
            ))),
        },
    }
}

#[pyclass(name = "NervusDb")]
pub struct PyNervusDb {
    inner: CoreNervusDb,
}

#[pymethods]
impl PyNervusDb {
    #[staticmethod]
    #[pyo3(signature = (path, pool_size = 1024))]
    pub fn open(path: &str, pool_size: usize) -> PyResult<Self> {
        let db = CoreNervusDb::open_with_pool_size(path, pool_size).map_err(to_py_err)?;
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
            .map_err(|e| NervusDbError::new_err(format!("dump is not valid UTF-8: {}", e)))
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
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))
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
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?
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
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?
            .add_edge(src, dst, edge_type, props, weight)
            .map_err(to_py_err)
    }

    /// 批量添加节点，返回按输入顺序排列的节点 ID 列表。
    ///
    /// ## 为什么需要它
    ///
    /// 逐条 `add_node` 每次都要跨越 Python/Rust 边界一次，并各自取一次全局写锁。
    /// 跨边界本身不贵，贵的是「每条记录的固定开销」：实测逐条写入约 63k ops/s，
    /// 而原生 Rust 路径在同一规模下约 550k ops/s——**差距主要来自调用次数**，
    /// 不是来自实际写入。
    ///
    /// 批量接口把 N 次跨边界与 N 次加解锁压成 1 次，把固定开销摊薄到整批上。
    ///
    /// ## 参数形式
    ///
    /// `nodes` 是一个可迭代对象，每项为 `(labels, properties)`：
    ///
    /// ```python
    /// with db.begin_transaction() as tx:
    ///     tx.add_nodes([(["Person"], {"name": "A"}), (["Person"], {"name": "B"})])
    /// ```
    ///
    /// 语义与逐条调用完全一致：同一个事务、同样的索引维护、同样的约束校验。
    /// 返回的 ID 列表与输入**顺序一一对应**，因此调用方可以据此建立自己的映射。
    #[pyo3(signature = (nodes))]
    pub fn add_nodes(&mut self, nodes: &Bound<'_, PyAny>) -> PyResult<Vec<u64>> {
        let parsed = parse_node_batch(nodes)?;
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?;

        // 走核心的批量路径：一次性预留全部 ID，避免每条记录取一次全局写锁
        tx.add_nodes(parsed).map_err(to_py_err)
    }

    /// 批量添加边，返回按输入顺序排列的边 ID 列表。
    ///
    /// 每项为 `(src, dst, edge_type, properties, weight)`，其中后两项可省略：
    ///
    /// ```python
    /// with db.begin_transaction() as tx:
    ///     tx.add_edges([(1, 2, "KNOWS", {"since": 2020}, 1.0),
    ///                   (2, 3, "KNOWS")])
    /// ```
    ///
    /// 与 `add_nodes` 同理：一次跨边界完成整批写入。整个批量在同一事务内，
    /// 由事务提交时统一 fsync。
    #[pyo3(signature = (edges))]
    pub fn add_edges(&mut self, edges: &Bound<'_, PyAny>) -> PyResult<Vec<u64>> {
        let parsed = parse_edge_batch(edges)?;
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?;

        // 与 add_nodes 同理：一次性预留全部 ID
        let inserts = parsed
            .into_iter()
            .map(
                |(src, dst, ty, props, weight)| nervusdb_core::disk_graph::EdgeInsert {
                    edge_id: 0, // 由核心在批量预留时填充
                    src_id: src,
                    dst_id: dst,
                    edge_type: ty,
                    properties: props,
                    weight,
                },
            )
            .collect();
        tx.add_edges(inserts).map_err(to_py_err)
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
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?
            .update_node_property(node_id, key, val)
            .map_err(|e| NervusDbError::new_err(e.to_string()))?;
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
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?
            .update_edge_property(edge_id, key, val)
            .map_err(|e| NervusDbError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn remove_node(&mut self, node_id: u64) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?
            .remove_node(node_id)
            .map_err(|e| NervusDbError::new_err(e.to_string()))?;
        Ok(())
    }

    pub fn remove_edge(&mut self, edge_id: u64) -> PyResult<()> {
        self.inner
            .as_mut()
            .ok_or_else(|| NervusDbError::new_err("transaction already finished"))?
            .remove_edge(edge_id)
            .map_err(|e| NervusDbError::new_err(e.to_string()))?;
        Ok(())
    }

    /// 提交事务：所有变更原子应用，仅触发一次 WAL fsync
    pub fn commit(&mut self) -> PyResult<()> {
        let tx = self.inner.take().ok_or_else(|| {
            NervusDbError::new_err("transaction already committed or rolled back")
        })?;
        tx.commit().map_err(to_py_err)
    }

    /// 回滚事务：丢弃全部未提交变更，主库零污染
    pub fn rollback(&mut self) -> PyResult<()> {
        let tx = self.inner.take().ok_or_else(|| {
            NervusDbError::new_err("transaction already committed or rolled back")
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
fn nervusdb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNervusDb>()?;
    m.add_class::<PyTransaction>()?;
    m.add("NervusDbError", m.py().get_type_bound::<NervusDbError>())?;
    Ok(())
}

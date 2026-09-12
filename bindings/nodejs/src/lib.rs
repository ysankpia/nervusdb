#[macro_use]
extern crate napi_derive;

use graphlite_core::{GraphLite as CoreGraphLite, Transaction as CoreTransaction, Value};
use std::collections::{HashMap, HashSet};

/// 把任意 JSON 标量转为图属性值
fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                Value::from(i)
            } else if let Some(f) = num.as_f64() {
                Value::from(f)
            } else {
                Value::from(num.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.clone()),
        other => Value::from(other.to_string()),
    }
}

fn json_to_properties(
    val_opt: Option<serde_json::Value>,
) -> Result<HashMap<String, Value>, napi::Error> {
    let mut map = HashMap::new();
    if let Some(serde_json::Value::Object(obj)) = val_opt {
        for (k, v) in obj {
            let prop_val = match v {
                serde_json::Value::Bool(b) => Value::from(b),
                serde_json::Value::Number(num) => {
                    if let Some(i) = num.as_i64() {
                        Value::from(i)
                    } else if let Some(f) = num.as_f64() {
                        Value::from(f)
                    } else {
                        Value::from(num.to_string())
                    }
                }
                serde_json::Value::String(s) => Value::from(s),
                _ => Value::from(v.to_string()),
            };
            map.insert(k, prop_val);
        }
    }
    Ok(map)
}

fn graph_value_to_json(v: &Value) -> serde_json::Value {
    match v {
        // null 直接映射为 JSON null；此前 null 用字符串 "null" 冒充，JS 侧
        // 拿到的会是字符串而不是 null
        Value::Null => serde_json::Value::Null,
        Value::Int(i) => serde_json::Value::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::from(s.clone()),
        Value::Bool(b) => serde_json::Value::from(*b),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(graph_value_to_json).collect())
        }
    }
}

#[napi(object)]
pub struct DijkstraResult {
    pub cost: f64,
    pub path: Vec<i64>,
}

#[napi(object)]
pub struct SubgraphEdge {
    pub id: i64,
    pub src_id: i64,
    pub dst_id: i64,
    pub edge_type: String,
    pub weight: f64,
}

#[napi(object)]
pub struct KHopSubgraph {
    pub nodes: Vec<i64>,
    pub edges: Vec<SubgraphEdge>,
}

fn parse_direction(direction: Option<&str>) -> Result<graphlite_core::Direction, napi::Error> {
    match direction.map(|d| d.to_ascii_lowercase()) {
        None => Ok(graphlite_core::Direction::Both),
        Some(d) => match d.as_str() {
            "out" | "outgoing" => Ok(graphlite_core::Direction::Outgoing),
            "in" | "incoming" => Ok(graphlite_core::Direction::Incoming),
            "both" | "any" => Ok(graphlite_core::Direction::Both),
            other => Err(napi::Error::from_reason(format!(
                "invalid direction '{}': expected outgoing/incoming/both",
                other
            ))),
        },
    }
}

#[napi(js_name = "GraphLite")]
pub struct JsGraphLite {
    inner: CoreGraphLite,
}

#[napi]
impl JsGraphLite {
    #[napi(factory)]
    pub fn open(path: String, pool_size: Option<u32>) -> Result<Self, napi::Error> {
        let frames = pool_size.unwrap_or(1024) as usize;
        let db = CoreGraphLite::open_with_pool_size(path, frames)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Self { inner: db })
    }

    #[napi]
    pub fn execute(&self, cypher: String) -> Result<serde_json::Value, napi::Error> {
        let res = self
            .inner
            .execute(&cypher)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let obj = serde_json::json!({
            "nodes_created": res.nodes_created,
            "edges_created": res.edges_created,
            "nodes_deleted": res.nodes_deleted,
            "edges_deleted": res.edges_deleted,
            "properties_set": res.properties_set,
            "message": res.message,
        });
        Ok(obj)
    }

    #[napi]
    pub fn query(&self, cypher: String) -> Result<Vec<serde_json::Value>, napi::Error> {
        let res = self
            .inner
            .query_cypher(&cypher)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let mut rows = Vec::new();
        for row in res.rows {
            let mut map = serde_json::Map::new();
            for (idx, col_name) in res.columns.iter().enumerate() {
                if let Some(val) = row.values.get(idx) {
                    map.insert(col_name.clone(), graph_value_to_json(val));
                } else {
                    map.insert(col_name.clone(), serde_json::Value::Null);
                }
            }
            rows.push(serde_json::Value::Object(map));
        }
        Ok(rows)
    }

    #[napi]
    pub fn add_node(
        &self,
        labels: Vec<String>,
        properties: Option<serde_json::Value>,
    ) -> Result<i64, napi::Error> {
        let labels_set: HashSet<String> = labels.into_iter().collect();
        let props = json_to_properties(properties)?;
        let id = self
            .inner
            .add_node(labels_set, props)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(id as i64)
    }

    #[napi]
    pub fn add_edge(
        &self,
        src: i64,
        dst: i64,
        edge_type: String,
        properties: Option<serde_json::Value>,
        weight: Option<f64>,
    ) -> Result<i64, napi::Error> {
        let props = json_to_properties(properties)?;
        let w = weight.unwrap_or(1.0);
        let id = self
            .inner
            .add_edge(src as u64, dst as u64, &edge_type, props, w)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(id as i64)
    }

    #[napi]
    pub fn dijkstra(
        &self,
        start: i64,
        end: i64,
        edge_type: Option<String>,
    ) -> Result<Option<DijkstraResult>, napi::Error> {
        let res = self
            .inner
            .dijkstra(start as u64, end as u64, edge_type.as_deref());
        Ok(res.map(|(cost, path)| DijkstraResult {
            cost,
            path: path.into_iter().map(|id| id as i64).collect(),
        }))
    }

    #[napi]
    pub fn stats(&self) -> Result<serde_json::Value, napi::Error> {
        let s = self.inner.buffer_stats();
        let obj = serde_json::json!({
            "capacity_frames": s.capacity_frames,
            "used_frames": s.used_frames,
            "dirty_frames": s.dirty_frames,
            "cache_hits": s.cache_hits,
            "cache_misses": s.cache_misses,
            "hit_rate_percentage": s.hit_rate_percentage,
            "disk_reads": s.disk_reads,
            "disk_writes": s.disk_writes,
            "file_size_bytes": s.file_size_bytes,
            "wal_page_count": s.wal_page_count,
            "spill_count": s.spill_count,
            "wal_size_bytes": s.wal_size_bytes,
            "wal_fsync_count": s.wal_fsync_count,
            "wal_frames_written": s.wal_frames_written,
        });
        Ok(obj)
    }

    /// 无权 BFS 最短路径
    #[napi]
    pub fn bfs(
        &self,
        start: i64,
        end: i64,
        edge_type: Option<String>,
    ) -> Result<Option<Vec<i64>>, napi::Error> {
        Ok(self
            .inner
            .bfs(start as u64, end as u64, edge_type.as_deref())
            .map(|path| path.into_iter().map(|id| id as i64).collect()))
    }

    #[napi]
    pub fn has_cycle(&self) -> Result<bool, napi::Error> {
        Ok(self.inner.has_cycle())
    }

    /// PageRank 阻尼迭代：[{ node_id, score }, ...]，按分数降序
    #[napi(js_name = "pageRank")]
    pub fn pagerank(
        &self,
        damping_factor: Option<f64>,
        max_iterations: Option<u32>,
        tolerance: Option<f64>,
    ) -> Result<Vec<serde_json::Value>, napi::Error> {
        let scores = self.inner.pagerank_with(
            damping_factor.unwrap_or(0.85),
            max_iterations.unwrap_or(100) as usize,
            tolerance.unwrap_or(1e-6),
        );
        Ok(scores
            .into_iter()
            .map(|s| serde_json::json!({ "node_id": s.node_id, "score": s.score }))
            .collect())
    }

    /// 弱连通分量：[[node_id, ...], ...]，按分量规模降序
    #[napi]
    pub fn weakly_connected_components(&self) -> Result<Vec<Vec<i64>>, napi::Error> {
        Ok(self
            .inner
            .weakly_connected_components()
            .into_iter()
            .map(|c| c.into_iter().map(|id| id as i64).collect())
            .collect())
    }

    /// K-Hop 局部子图提取
    #[napi]
    pub fn k_hop_subgraph(
        &self,
        start: i64,
        k: u32,
        direction: Option<String>,
        edge_type: Option<String>,
    ) -> Result<KHopSubgraph, napi::Error> {
        let dir = parse_direction(direction.as_deref())?;
        let sub = self
            .inner
            .k_hop_subgraph_with(start as u64, k as usize, dir, edge_type.as_deref())
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(KHopSubgraph {
            nodes: sub.nodes.into_iter().map(|id| id as i64).collect(),
            edges: sub
                .edges
                .into_iter()
                .map(|e| SubgraphEdge {
                    id: e.id as i64,
                    src_id: e.src_id as i64,
                    dst_id: e.dst_id as i64,
                    edge_type: e.edge_type,
                    weight: e.weight,
                })
                .collect(),
        })
    }

    /// 图模式：全部标签
    #[napi]
    pub fn labels(&self) -> Result<Vec<String>, napi::Error> {
        Ok(self.inner.labels())
    }

    #[napi]
    pub fn edge_types(&self) -> Result<Vec<String>, napi::Error> {
        Ok(self.inner.edge_types())
    }

    /// 导出当前图数据为可回灌的 Cypher 脚本
    #[napi]
    pub fn dump_cypher(&self) -> Result<String, napi::Error> {
        let mut buffer: Vec<u8> = Vec::new();
        self.inner
            .dump_cypher(&mut buffer)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        String::from_utf8(buffer)
            .map_err(|e| napi::Error::from_reason(format!("dump is not valid UTF-8: {}", e)))
    }

    #[napi]
    pub fn checkpoint(&self) -> Result<(), napi::Error> {
        self.inner
            .checkpoint()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }

    /// 开启显式批量事务：脏页在 commit() 时批量写入 WAL 并**只做一次 fsync**
    #[napi(js_name = "beginTransaction")]
    pub fn begin_transaction(&self) -> Result<Transaction, napi::Error> {
        let inner = self
            .inner
            .begin_transaction()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Transaction { inner: Some(inner) })
    }
}

/// 显式批量事务句柄
#[napi]
pub struct Transaction {
    inner: Option<CoreTransaction>,
}

#[napi]
impl Transaction {
    #[napi]
    pub fn tx_id(&self) -> Result<i64, napi::Error> {
        self.inner
            .as_ref()
            .map(|tx| tx.tx_id() as i64)
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))
    }

    #[napi]
    pub fn add_node(
        &mut self,
        labels: Vec<String>,
        properties: Option<serde_json::Value>,
    ) -> Result<i64, napi::Error> {
        let labels_set: HashSet<String> = labels.into_iter().collect();
        let props = json_to_properties(properties)?;
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?;
        let id = tx
            .add_node(labels_set, props)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(id as i64)
    }

    #[napi]
    pub fn add_edge(
        &mut self,
        src: i64,
        dst: i64,
        edge_type: String,
        properties: Option<serde_json::Value>,
        weight: Option<f64>,
    ) -> Result<i64, napi::Error> {
        let props = json_to_properties(properties)?;
        let w = weight.unwrap_or(1.0);
        let tx = self
            .inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?;
        let id = tx
            .add_edge(src as u64, dst as u64, &edge_type, props, w)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(id as i64)
    }

    #[napi]
    pub fn update_node_property(
        &mut self,
        node_id: i64,
        key: String,
        value: serde_json::Value,
    ) -> Result<(), napi::Error> {
        let val = json_to_value(&value);
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .update_node_property(node_id as u64, key, val);
        Ok(())
    }

    #[napi]
    pub fn update_edge_property(
        &mut self,
        edge_id: i64,
        key: String,
        value: serde_json::Value,
    ) -> Result<(), napi::Error> {
        let val = json_to_value(&value);
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .update_edge_property(edge_id as u64, key, val);
        Ok(())
    }

    #[napi]
    pub fn remove_node(&mut self, node_id: i64) -> Result<(), napi::Error> {
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .remove_node(node_id as u64);
        Ok(())
    }

    #[napi]
    pub fn remove_edge(&mut self, edge_id: i64) -> Result<(), napi::Error> {
        self.inner
            .as_mut()
            .ok_or_else(|| napi::Error::from_reason("transaction already finished"))?
            .remove_edge(edge_id as u64);
        Ok(())
    }

    /// 提交事务：所有变更原子应用，仅触发一次 WAL fsync
    #[napi]
    pub fn commit(&mut self) -> Result<(), napi::Error> {
        let tx = self.inner.take().ok_or_else(|| {
            napi::Error::from_reason("transaction already committed or rolled back")
        })?;
        tx.commit()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }

    /// 回滚事务：丢弃全部未提交变更，主库零污染
    #[napi]
    pub fn rollback(&mut self) -> Result<(), napi::Error> {
        let tx = self.inner.take().ok_or_else(|| {
            napi::Error::from_reason("transaction already committed or rolled back")
        })?;
        tx.rollback()
            .map_err(|e| napi::Error::from_reason(e.to_string()))
    }
}

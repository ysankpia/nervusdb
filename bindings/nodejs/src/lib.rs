#[macro_use]
extern crate napi_derive;

use graphlite_core::{GraphLite as CoreGraphLite, Value};
use std::collections::{HashMap, HashSet};

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
        Value::Int(i) => serde_json::Value::from(*i),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::from(s.clone()),
        Value::Bool(b) => serde_json::Value::from(*b),
    }
}

#[napi(object)]
pub struct DijkstraResult {
    pub cost: f64,
    pub path: Vec<i64>,
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
        });
        Ok(obj)
    }

    #[napi]
    pub fn checkpoint(&self) -> Result<(), napi::Error> {
        self.inner
            .checkpoint()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(())
    }
}

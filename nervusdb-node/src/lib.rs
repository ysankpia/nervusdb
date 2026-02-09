use napi::bindgen_prelude::Result;
use napi::Error;
use napi_derive::napi;
use nervusdb_v2::Db as RustDb;
use nervusdb_v2_query::{Params, Value};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

fn napi_err(err: impl ToString) -> Error {
    Error::from_reason(err.to_string())
}

fn value_to_json(v: Value) -> JsonValue {
    match v {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => json!(f),
        Value::String(s) => json!(s),
        Value::DateTime(ts) => json!({"type": "datetime", "value": ts}),
        Value::Blob(bytes) => json!({"type": "blob", "len": bytes.len()}),
        Value::List(list) => JsonValue::Array(list.into_iter().map(value_to_json).collect()),
        Value::Map(map) => {
            let mut out = JsonMap::new();
            for (k, v) in map {
                out.insert(k, value_to_json(v));
            }
            JsonValue::Object(out)
        }
        Value::Node(n) => {
            let mut props = JsonMap::new();
            for (k, v) in n.properties {
                props.insert(k, value_to_json(v));
            }
            json!({
                "type": "node",
                "id": n.id,
                "labels": n.labels,
                "properties": props,
            })
        }
        Value::Relationship(r) => {
            let mut props = JsonMap::new();
            for (k, v) in r.properties {
                props.insert(k, value_to_json(v));
            }
            json!({
                "type": "relationship",
                "src": r.key.src,
                "dst": r.key.dst,
                "rel_type": r.rel_type,
                "properties": props,
            })
        }
        Value::ReifiedPath(p) => {
            let nodes = p.nodes.into_iter().map(Value::Node).map(value_to_json).collect::<Vec<_>>();
            let rels = p
                .relationships
                .into_iter()
                .map(Value::Relationship)
                .map(value_to_json)
                .collect::<Vec<_>>();
            json!({"type": "path", "nodes": nodes, "relationships": rels})
        }
        Value::NodeId(id) => json!({"type": "node_id", "value": id}),
        Value::ExternalId(id) => json!({"type": "external_id", "value": id}),
        Value::EdgeKey(k) => json!({"type": "edge_key", "src": k.src, "dst": k.dst}),
        Value::Path(p) => {
            let edges = p.edges.into_iter().map(|e| json!({"src": e.src, "dst": e.dst})).collect::<Vec<_>>();
            json!({"type": "path_legacy", "nodes": p.nodes, "edges": edges})
        }
    }
}

fn run_query(db: &RustDb, cypher: &str) -> std::result::Result<Vec<JsonValue>, String> {
    let prepared = nervusdb_v2_query::prepare(cypher).map_err(|e| e.to_string())?;
    let snapshot = db.snapshot();
    let rows: Vec<_> = prepared
        .execute_streaming(&snapshot, &Params::new())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut map = JsonMap::new();
        for (col, val) in row.columns() {
            let rv = val.reify(&snapshot).map_err(|e| e.to_string())?;
            map.insert(col.clone(), value_to_json(rv));
        }
        out.push(JsonValue::Object(map));
    }
    Ok(out)
}

#[napi]
pub struct Db {
    inner: Option<RustDb>,
    path: String,
}

#[napi]
impl Db {
    #[napi(factory)]
    pub fn open(path: String) -> Result<Self> {
        let inner = RustDb::open(&path).map_err(napi_err)?;
        Ok(Self {
            inner: Some(inner),
            path,
        })
    }

    #[napi]
    pub fn query(&self, cypher: String) -> Result<Vec<JsonValue>> {
        let db = self
            .inner
            .as_ref()
            .ok_or_else(|| napi_err("database is closed"))?;
        run_query(db, &cypher).map_err(napi_err)
    }

    #[napi]
    pub fn execute_write(&self, cypher: String) -> Result<u32> {
        let db = self
            .inner
            .as_ref()
            .ok_or_else(|| napi_err("database is closed"))?;
        let prepared = nervusdb_v2_query::prepare(&cypher).map_err(napi_err)?;
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();
        let created = prepared
            .execute_write(&snapshot, &mut txn, &Params::new())
            .map_err(napi_err)?;
        txn.commit().map_err(napi_err)?;
        Ok(created)
    }

    #[napi]
    pub fn begin_write(&self) -> Result<WriteTxn> {
        if self.inner.is_none() {
            return Err(napi_err("database is closed"));
        }
        Ok(WriteTxn {
            db_path: self.path.clone(),
            staged_queries: Vec::new(),
        })
    }

    #[napi]
    pub fn close(&mut self) {
        if let Some(inner) = self.inner.take() {
            let _ = inner.close();
        }
    }
}

#[napi]
pub struct WriteTxn {
    db_path: String,
    staged_queries: Vec<String>,
}

#[napi]
impl WriteTxn {
    #[napi]
    pub fn query(&mut self, cypher: String) -> Result<()> {
        // compile fast-fail at enqueue time
        let _ = nervusdb_v2_query::prepare(&cypher).map_err(napi_err)?;
        self.staged_queries.push(cypher);
        Ok(())
    }

    #[napi]
    pub fn rollback(&mut self) {
        self.staged_queries.clear();
    }

    #[napi]
    pub fn commit(&mut self) -> Result<u32> {
        let db = RustDb::open(&self.db_path).map_err(napi_err)?;
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();

        let mut total = 0u32;
        for cypher in self.staged_queries.drain(..) {
            let prepared = nervusdb_v2_query::prepare(&cypher).map_err(napi_err)?;
            total += prepared
                .execute_write(&snapshot, &mut txn, &Params::new())
                .map_err(napi_err)?;
        }

        txn.commit().map_err(napi_err)?;
        Ok(total)
    }
}

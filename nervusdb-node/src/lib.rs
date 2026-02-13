use napi::bindgen_prelude::Result;
use napi::Error;
use napi_derive::napi;
use nervusdb::Db as RustDb;
use nervusdb::Error as V2Error;
use nervusdb_query::{Params, Value};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

fn error_payload(code: &str, category: &str, message: impl ToString) -> String {
    json!({
        "code": code,
        "category": category,
        "message": message.to_string(),
    })
    .to_string()
}

fn classify_err_message(msg: &str) -> (&'static str, &'static str) {
    let lower = msg.to_lowercase();
    if lower.contains("storage format mismatch") || lower.contains("compatibility") {
        ("NERVUS_COMPATIBILITY", "compatibility")
    } else if lower.contains("syntax")
        || lower.contains("parse")
        || lower.contains("unexpected token")
        || lower.starts_with("expected ")
    {
        ("NERVUS_SYNTAX", "syntax")
    } else if lower.contains("wal")
        || lower.contains("checkpoint")
        || lower.contains("io error")
        || lower.contains("permission denied")
        || lower.contains("no such file")
        || lower.contains("disk full")
        || lower.contains("database is closed")
    {
        ("NERVUS_STORAGE", "storage")
    } else {
        ("NERVUS_EXECUTION", "execution")
    }
}

fn napi_err(err: impl ToString) -> Error {
    let message = err.to_string();
    let (code, category) = classify_err_message(&message);
    Error::from_reason(error_payload(code, category, message))
}

fn napi_err_v2(err: V2Error) -> Error {
    let payload = match err {
        V2Error::Compatibility(message) => {
            error_payload("NERVUS_COMPATIBILITY", "compatibility", message)
        }
        V2Error::Storage(message) => error_payload("NERVUS_STORAGE", "storage", message),
        V2Error::Query(message) => error_payload("NERVUS_EXECUTION", "execution", message),
        V2Error::Other(message) => error_payload("NERVUS_EXECUTION", "execution", message),
        V2Error::Io(io_err) => error_payload("NERVUS_STORAGE", "storage", io_err),
    };
    Error::from_reason(payload)
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
    let prepared = nervusdb_query::prepare(cypher).map_err(|e| e.to_string())?;
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
        let inner = RustDb::open(&path).map_err(napi_err_v2)?;
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
        let prepared = nervusdb_query::prepare(&cypher).map_err(napi_err)?;
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
        let _ = nervusdb_query::prepare(&cypher).map_err(napi_err)?;
        self.staged_queries.push(cypher);
        Ok(())
    }

    #[napi]
    pub fn rollback(&mut self) {
        self.staged_queries.clear();
    }

    #[napi]
    pub fn commit(&mut self) -> Result<u32> {
        let db = RustDb::open(&self.db_path).map_err(napi_err_v2)?;
        let snapshot = db.snapshot();
        let mut txn = db.begin_write();

        let mut total = 0u32;
        for cypher in self.staged_queries.drain(..) {
            let prepared = nervusdb_query::prepare(&cypher).map_err(napi_err)?;
            total += prepared
                .execute_write(&snapshot, &mut txn, &Params::new())
                .map_err(napi_err)?;
        }

        txn.commit().map_err(napi_err)?;
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_err_message, napi_err};
    use serde_json::Value;

    fn parse_payload(reason: &str) -> Value {
        serde_json::from_str(reason).expect("napi reason should be valid json payload")
    }

    #[test]
    fn napi_err_uses_structured_compatibility_payload() {
        let err = napi_err("storage format mismatch: expected epoch 1, found 0");
        let reason = err.reason;
        assert!(reason.contains("\"category\":\"compatibility\""));
        assert!(reason.contains("\"code\":\"NERVUS_COMPATIBILITY\""));
    }

    #[test]
    fn napi_err_maps_syntax_messages_to_syntax_category() {
        let err = napi_err("syntax error: unexpected token");
        let payload = parse_payload(&err.reason);
        assert_eq!(payload["code"], "NERVUS_SYNTAX");
        assert_eq!(payload["category"], "syntax");
        assert_eq!(payload["message"], "syntax error: unexpected token");
    }

    #[test]
    fn napi_err_maps_expected_prefix_to_syntax_category() {
        let err = napi_err("Expected ')'");
        let payload = parse_payload(&err.reason);
        assert_eq!(payload["code"], "NERVUS_SYNTAX");
        assert_eq!(payload["category"], "syntax");
    }

    #[test]
    fn napi_err_maps_storage_messages_for_fs_failures() {
        let err = napi_err("permission denied while opening wal");
        let payload = parse_payload(&err.reason);
        assert_eq!(payload["code"], "NERVUS_STORAGE");
        assert_eq!(payload["category"], "storage");
    }

    #[test]
    fn napi_err_falls_back_to_execution_category() {
        let err = napi_err("not implemented: expression");
        let payload = parse_payload(&err.reason);
        assert_eq!(payload["code"], "NERVUS_EXECUTION");
        assert_eq!(payload["category"], "execution");
    }

    #[test]
    fn classify_err_message_keeps_compatibility_priority() {
        let (code, category) =
            classify_err_message("compatibility warning with parse token details");
        assert_eq!(code, "NERVUS_COMPATIBILITY");
        assert_eq!(category, "compatibility");
    }
}

//! v2 Query API bindings for UniFFI
//!
//! This module provides v2 query engine bindings when the "v2" feature is enabled.

use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use crate::V2Error;
use nervusdb_v2::Db as V2DatabaseCore;
use nervusdb_v2_query::facade::query_collect;
use nervusdb_v2_query::Params;

enum Command {
    Query {
        cypher: String,
        params_json: Option<String>,
        reply: mpsc::Sender<Result<V2QueryResult, V2Error>>,
    },
    Close {
        reply: mpsc::Sender<()>,
    },
}

struct WorkerState {
    db: Option<V2DatabaseCore>,
}

fn worker_main(
    path: String,
    rx: mpsc::Receiver<Command>,
    init: mpsc::Sender<Result<(), V2Error>>,
) {
    let db = match V2DatabaseCore::open(&path) {
        Ok(db) => Some(db),
        Err(err) => {
            let _ = init.send(Err(V2Error::from(err)));
            return;
        }
    };

    let mut state = WorkerState { db };

    let _ = init.send(Ok(()));

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::Query {
                cypher,
                params_json,
                reply,
            } => {
                let res = query_impl(&mut state, cypher, params_json);
                let _ = reply.send(res);
            }
            Command::Close { reply } => {
                state.db.take();
                let _ = reply.send(());
                break;
            }
        }
    }
}

fn parse_params(params_json: Option<String>) -> Result<Params, V2Error> {
    match params_json {
        None => Ok(Params::new()),
        Some(raw) => {
            if raw.trim().is_empty() {
                Ok(Params::new())
            } else {
                let map: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&raw).map_err(|_| V2Error::Other {
                        message: "params_json must be a JSON object".to_string(),
                    })?;
                let mut params = Params::new();
                for (k, v) in map {
                    params.insert(k, json_to_query_value(v));
                }
                Ok(params)
            }
        }
    }
}

fn json_to_query_value(json: serde_json::Value) -> nervusdb_v2_query::Value {
    match json {
        serde_json::Value::Null => nervusdb_v2_query::Value::Null,
        serde_json::Value::Bool(b) => nervusdb_v2_query::Value::Bool(b),
        serde_json::Value::Number(n) => {
            if n.is_i64() {
                nervusdb_v2_query::Value::Int(n.as_i64().unwrap_or(0))
            } else {
                nervusdb_v2_query::Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => nervusdb_v2_query::Value::String(s),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            nervusdb_v2_query::Value::String(json.to_string())
        }
    }
}

fn query_impl(
    state: &mut WorkerState,
    cypher: String,
    params_json: Option<String>,
) -> Result<V2QueryResult, V2Error> {
    let db = state.db.as_ref().ok_or(V2Error::Other {
        message: "database is closed".to_string(),
    })?;
    let snapshot = db.snapshot();
    let params = parse_params(params_json)?;

    let rows = query_collect(&snapshot, &cypher, &params).map_err(V2Error::from)?;

    Ok(V2QueryResult::new(rows))
}

struct WorkerHandle {
    tx: mpsc::Sender<Command>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl WorkerHandle {
    fn close(&self) {
        let (reply_tx, reply_rx) = mpsc::channel();
        let _ = self.tx.send(Command::Close { reply: reply_tx });
        let _ = reply_rx.recv();
        if let Some(join) = self.join.lock().ok().and_then(|mut g| g.take()) {
            let _ = join.join();
        }
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.close();
    }
}

/// v2 Database handle for Python bindings
pub struct V2Database {
    worker: Arc<WorkerHandle>,
}

impl V2Database {
    pub fn new(path: String) -> Result<Self, V2Error> {
        let (tx, rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();
        let join = std::thread::spawn(move || worker_main(path, rx, init_tx));

        init_rx.recv().map_err(|_| V2Error::Other {
            message: "worker thread is not available".to_string(),
        })??;

        Ok(Self {
            worker: Arc::new(WorkerHandle {
                tx,
                join: Mutex::new(Some(join)),
            }),
        })
    }

    pub fn query(&self, cypher: String, params_json: Option<String>) -> Result<Arc<V2QueryResult>, V2Error> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.worker
            .tx
            .send(Command::Query {
                cypher,
                params_json,
                reply: reply_tx,
            })
            .map_err(|_| V2Error::Other {
                message: "worker thread is not available".to_string(),
            })?;
        let result = reply_rx.recv().map_err(|_| V2Error::Other {
            message: "worker thread is not available".to_string(),
        })??;
        Ok(Arc::new(result))
    }

    pub fn close(&self) {
        self.worker.close();
    }
}

/// Represents a row of query results
pub struct V2Row {
    columns: Vec<String>,
    values: Vec<V2Value>,
}

impl V2Row {
    fn new(columns: Vec<String>, values: Vec<V2Value>) -> Self {
        Self { columns, values }
    }

    pub fn column_count(&self) -> u32 {
        self.columns.len() as u32
    }

    pub fn column_name(&self, index: u32) -> Option<String> {
        self.columns.get(index as usize).cloned()
    }

    pub fn get_text(&self, index: u32) -> Option<String> {
        match self.values.get(index as usize) {
            Some(V2Value::Text(s)) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn get_float(&self, index: u32) -> Option<f64> {
        match self.values.get(index as usize) {
            Some(V2Value::Float(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_int(&self, index: u32) -> Option<i64> {
        match self.values.get(index as usize) {
            Some(V2Value::Int(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_bool(&self, index: u32) -> Option<bool> {
        match self.values.get(index as usize) {
            Some(V2Value::Bool(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn get_null(&self, index: u32) -> bool {
        matches!(self.values.get(index as usize), Some(V2Value::Null))
    }
}

/// Value types for v2 queries
#[derive(Debug, Clone)]
pub enum V2Value {
    Null,
    Text(String),
    Float(f64),
    Int(i64),
    Bool(bool),
}

impl From<nervusdb_v2_query::Value> for V2Value {
    fn from(value: nervusdb_v2_query::Value) -> Self {
        match value {
            nervusdb_v2_query::Value::Null => V2Value::Null,
            nervusdb_v2_query::Value::String(s) => V2Value::Text(s),
            nervusdb_v2_query::Value::Float(f) => V2Value::Float(f),
            nervusdb_v2_query::Value::Int(i) => V2Value::Int(i),
            nervusdb_v2_query::Value::Bool(b) => V2Value::Bool(b),
            nervusdb_v2_query::Value::NodeId(_) => V2Value::Text("NodeId".to_string()),
            nervusdb_v2_query::Value::ExternalId(_) => V2Value::Text("ExternalId".to_string()),
            nervusdb_v2_query::Value::EdgeKey(_) => V2Value::Text("EdgeKey".to_string()),
        }
    }
}

/// Query result container (holds all rows in memory for MVP)
pub struct V2QueryResult {
    rows: Vec<V2Row>,
    current_index: Mutex<usize>,
}

impl V2QueryResult {
    fn new(rows: Vec<nervusdb_v2_query::Row>) -> Self {
        let converted_rows: Vec<V2Row> = rows
            .into_iter()
            .map(|row| {
                let mut columns = Vec::new();
                let mut values = Vec::new();
                for (name, val) in row.columns().iter().cloned() {
                    columns.push(name);
                    values.push(V2Value::from(val));
                }
                V2Row::new(columns, values)
            })
            .collect();

        Self {
            rows: converted_rows,
            current_index: Mutex::new(0),
        }
    }

    pub fn row_count(&self) -> Result<u32, V2Error> {
        Ok(self.rows.len() as u32)
    }

    pub fn step(&self) -> bool {
        let mut index = self.current_index.lock().unwrap();
        if *index < self.rows.len() {
            *index += 1;
            true
        } else {
            false
        }
    }

    pub fn get_text(&self, index: u32) -> Result<Option<String>, V2Error> {
        let idx = self.current_index.lock().unwrap().saturating_sub(1);
        Ok(self.rows.get(idx).and_then(|r| r.get_text(index)))
    }

    pub fn get_float(&self, index: u32) -> Result<Option<f64>, V2Error> {
        let idx = self.current_index.lock().unwrap().saturating_sub(1);
        Ok(self.rows.get(idx).and_then(|r| r.get_float(index)))
    }

    pub fn get_int(&self, index: u32) -> Result<Option<i64>, V2Error> {
        let idx = self.current_index.lock().unwrap().saturating_sub(1);
        Ok(self.rows.get(idx).and_then(|r| r.get_int(index)))
    }

    pub fn get_bool(&self, index: u32) -> Result<Option<bool>, V2Error> {
        let idx = self.current_index.lock().unwrap().saturating_sub(1);
        Ok(self.rows.get(idx).and_then(|r| r.get_bool(index)))
    }

    pub fn get_null(&self, index: u32) -> Result<bool, V2Error> {
        let idx = self.current_index.lock().unwrap().saturating_sub(1);
        Ok(self.rows.get(idx).map_or(false, |r| r.get_null(index)))
    }
}

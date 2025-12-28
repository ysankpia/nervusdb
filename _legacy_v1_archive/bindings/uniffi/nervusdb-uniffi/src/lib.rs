#![allow(clippy::empty_line_after_doc_comments)]

use nervusdb_core::query::executor::{Record, Value as ExecValue};
use nervusdb_core::query::planner::QueryPlanner;
use nervusdb_core::{Database as CoreDatabase, Error as CoreError, Fact, Options, Triple};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

uniffi::include_scaffolding!("nervusdb");

#[cfg(not(feature = "v2"))]
#[derive(Debug)]
pub enum V2Error {
    Other { message: String },
}

#[cfg(not(feature = "v2"))]
impl std::fmt::Display for V2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            V2Error::Other { message } => write!(f, "{message}"),
        }
    }
}

#[cfg(not(feature = "v2"))]
impl std::error::Error for V2Error {}

#[derive(Debug)]
pub enum NervusError {
    Io { message: String },
    InvalidArgument { message: String },
    NotImplemented { message: String },
    NotFound,
    Other { message: String },
}

impl std::fmt::Display for NervusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { message } => write!(f, "io: {message}"),
            Self::InvalidArgument { message } => write!(f, "invalid argument: {message}"),
            Self::NotImplemented { message } => write!(f, "not implemented: {message}"),
            Self::NotFound => write!(f, "not found"),
            Self::Other { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for NervusError {}

impl From<CoreError> for NervusError {
    fn from(value: CoreError) -> Self {
        match value {
            CoreError::Io(err) => Self::Io {
                message: err.to_string(),
            },
            CoreError::NotImplemented(message) => Self::NotImplemented {
                message: message.to_string(),
            },
            CoreError::NotFound => Self::NotFound,
            CoreError::UnknownString(s) => Self::Other {
                message: format!("unknown string: {s}"),
            },
            CoreError::InvalidCursor(id) => Self::Other {
                message: format!("invalid cursor id: {id}"),
            },
            CoreError::Other(message) => Self::Other { message },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Null,
    Text,
    Float,
    Bool,
    Node,
    Relationship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relationship {
    pub subject_id: u64,
    pub predicate_id: u64,
    pub object_id: u64,
}

impl From<Triple> for Relationship {
    fn from(value: Triple) -> Self {
        Self {
            subject_id: value.subject_id,
            predicate_id: value.predicate_id,
            object_id: value.object_id,
        }
    }
}

#[derive(Debug, Clone)]
enum WireValue {
    Null,
    Text(String),
    Float(f64),
    Bool(bool),
    Node(u64),
    Relationship(Relationship),
}

impl WireValue {
    fn value_type(&self) -> ValueType {
        match self {
            Self::Null => ValueType::Null,
            Self::Text(_) => ValueType::Text,
            Self::Float(_) => ValueType::Float,
            Self::Bool(_) => ValueType::Bool,
            Self::Node(_) => ValueType::Node,
            Self::Relationship(_) => ValueType::Relationship,
        }
    }
}

fn wire_from_exec(value: ExecValue) -> WireValue {
    match value {
        ExecValue::Null => WireValue::Null,
        ExecValue::String(s) => WireValue::Text(s),
        ExecValue::Float(f) => WireValue::Float(f),
        ExecValue::Boolean(b) => WireValue::Bool(b),
        ExecValue::Node(id) => WireValue::Node(id),
        ExecValue::Relationship(triple) => WireValue::Relationship(Relationship::from(triple)),
        ExecValue::Vector(values) => {
            WireValue::Text(serde_json::to_string(&values).unwrap_or("[]".to_string()))
        }
    }
}

fn infer_projection_alias(expr: &nervusdb_core::query::ast::Expression) -> String {
    use nervusdb_core::query::ast::Expression;
    match expr {
        Expression::Variable(name) => name.clone(),
        Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
        _ => "col".to_string(),
    }
}

enum Command {
    PrepareV2 {
        query: String,
        params_json: Option<String>,
        reply: mpsc::Sender<Result<Prepared, NervusError>>,
    },
    AddFact {
        subject: String,
        predicate: String,
        object: String,
        reply: mpsc::Sender<Result<Relationship, NervusError>>,
    },
    NextBatch {
        statement_id: u64,
        max_rows: usize,
        reply: mpsc::Sender<Result<NextBatch, NervusError>>,
    },
    Finalize {
        statement_id: u64,
    },
    Close {
        reply: mpsc::Sender<()>,
    },
}

struct Prepared {
    statement_id: u64,
    column_names: Vec<String>,
}

struct NextBatch {
    rows: Vec<Vec<WireValue>>,
    done: bool,
}

struct WorkerState {
    db: Arc<CoreDatabase>,
    next_statement_id: u64,
    statements: HashMap<u64, StatementState>,
}

struct StatementState {
    column_names: Vec<String>,
    iterator: Box<dyn Iterator<Item = Result<Record, CoreError>> + 'static>,
    done: bool,
}

fn worker_main(
    path: String,
    rx: mpsc::Receiver<Command>,
    init: mpsc::Sender<Result<(), NervusError>>,
) {
    let db = match CoreDatabase::open(Options::new(path)).map_err(NervusError::from) {
        Ok(db) => db,
        Err(err) => {
            let _ = init.send(Err(err));
            return;
        }
    };

    // NOTE: `Arc<Database>` here is for shared ownership with statements, not thread safety.
    #[allow(clippy::arc_with_non_send_sync)]
    let db = Arc::new(db);

    let mut state = WorkerState {
        db,
        next_statement_id: 1,
        statements: HashMap::new(),
    };

    let _ = init.send(Ok(()));

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::PrepareV2 {
                query,
                params_json,
                reply,
            } => {
                let res = prepare_v2_impl(&mut state, query, params_json);
                let _ = reply.send(res);
            }
            Command::AddFact {
                subject,
                predicate,
                object,
                reply,
            } => {
                let res = add_fact_impl(&mut state, subject, predicate, object);
                let _ = reply.send(res);
            }
            Command::NextBatch {
                statement_id,
                max_rows,
                reply,
            } => {
                let res = next_batch_impl(&mut state, statement_id, max_rows);
                let _ = reply.send(res);
            }
            Command::Finalize { statement_id } => {
                state.statements.remove(&statement_id);
            }
            Command::Close { reply } => {
                state.statements.clear();
                let _ = reply.send(());
                break;
            }
        }
    }
}

fn prepare_v2_impl(
    state: &mut WorkerState,
    query: String,
    params_json: Option<String>,
) -> Result<Prepared, NervusError> {
    let params = match params_json {
        None => None,
        Some(raw) => {
            if raw.trim().is_empty() {
                None
            } else {
                Some(
                    serde_json::from_str::<HashMap<String, serde_json::Value>>(&raw).map_err(
                        |_| NervusError::InvalidArgument {
                            message: "params_json must be a JSON object".to_string(),
                        },
                    )?,
                )
            }
        }
    };

    let ast = nervusdb_core::query::parser::Parser::parse(query.as_str()).map_err(|err| {
        NervusError::InvalidArgument {
            message: err.to_string(),
        }
    })?;

    let mut projection_names: Vec<String> = Vec::new();
    for clause in &ast.clauses {
        if let nervusdb_core::query::ast::Clause::Return(r) = clause {
            projection_names = r
                .items
                .iter()
                .map(|item| {
                    item.alias
                        .clone()
                        .unwrap_or_else(|| infer_projection_alias(&item.expression))
                })
                .collect();
        }
    }

    if !projection_names.is_empty() {
        let mut seen = HashSet::new();
        for name in &projection_names {
            if !seen.insert(name) {
                return Err(NervusError::InvalidArgument {
                    message: format!("duplicate column name: {name}; use explicit aliases"),
                });
            }
        }
    }

    let planner = QueryPlanner::new();
    let plan = planner.plan(ast).map_err(NervusError::from)?;

    let param_values: HashMap<String, ExecValue> = params
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, CoreDatabase::serde_value_to_executor_value(v)))
        .collect();

    #[allow(clippy::arc_with_non_send_sync)]
    let ctx = Arc::new(nervusdb_core::query::executor::ArcExecutionContext::new(
        Arc::clone(&state.db),
        param_values,
    ));
    let iterator = plan.execute_streaming(ctx).map_err(NervusError::from)?;

    let statement_id = state.next_statement_id;
    state.next_statement_id += 1;
    state.statements.insert(
        statement_id,
        StatementState {
            column_names: projection_names.clone(),
            iterator,
            done: false,
        },
    );

    Ok(Prepared {
        statement_id,
        column_names: projection_names,
    })
}

fn add_fact_impl(
    state: &mut WorkerState,
    subject: String,
    predicate: String,
    object: String,
) -> Result<Relationship, NervusError> {
    // Mirror the C/Node behavior: unsafe mutable access under single-threaded worker ownership.
    let db = unsafe { &mut *(Arc::as_ptr(&state.db) as *mut CoreDatabase) };
    let triple = db
        .add_fact(Fact::new(&subject, &predicate, &object))
        .map_err(NervusError::from)?;
    Ok(Relationship::from(triple))
}

fn next_batch_impl(
    state: &mut WorkerState,
    statement_id: u64,
    max_rows: usize,
) -> Result<NextBatch, NervusError> {
    let Some(stmt) = state.statements.get_mut(&statement_id) else {
        return Err(NervusError::InvalidArgument {
            message: "statement already finalized".to_string(),
        });
    };

    if stmt.done {
        return Ok(NextBatch {
            rows: Vec::new(),
            done: true,
        });
    }

    let mut out = Vec::new();
    for _ in 0..max_rows.max(1) {
        match stmt.iterator.next() {
            Some(Ok(record)) => {
                let mut row = Vec::with_capacity(stmt.column_names.len());
                for col in &stmt.column_names {
                    let value = record.values.get(col).cloned().unwrap_or(ExecValue::Null);
                    row.push(wire_from_exec(value));
                }
                out.push(row);
            }
            Some(Err(err)) => return Err(NervusError::from(err)),
            None => {
                stmt.done = true;
                break;
            }
        }
    }

    Ok(NextBatch {
        done: stmt.done,
        rows: out,
    })
}

struct WorkerHandle {
    tx: mpsc::Sender<Command>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl WorkerHandle {
    fn close(&self) {
        let (reply_tx, reply_rx) = mpsc::channel();
        // If the worker is already down, ignore.
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

pub struct Database {
    worker: Arc<WorkerHandle>,
}

impl Database {
    pub fn new(path: String) -> Result<Self, NervusError> {
        let (tx, rx) = mpsc::channel();
        let (init_tx, init_rx) = mpsc::channel();
        let join = std::thread::spawn(move || worker_main(path, rx, init_tx));

        init_rx.recv().map_err(|_| NervusError::Other {
            message: "worker thread is not available".to_string(),
        })??;

        Ok(Self {
            worker: Arc::new(WorkerHandle {
                tx,
                join: Mutex::new(Some(join)),
            }),
        })
    }

    pub fn prepare_v2(
        &self,
        cypher: String,
        params_json: Option<String>,
    ) -> Result<Arc<Statement>, NervusError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.worker
            .tx
            .send(Command::PrepareV2 {
                query: cypher,
                params_json,
                reply: reply_tx,
            })
            .map_err(|_| NervusError::Other {
                message: "worker thread is not available".to_string(),
            })?;
        let prepared = reply_rx.recv().map_err(|_| NervusError::Other {
            message: "worker thread is not available".to_string(),
        })??;
        Ok(Arc::new(Statement::new(
            Arc::clone(&self.worker),
            prepared.statement_id,
            prepared.column_names,
        )))
    }

    pub fn add_fact(
        &self,
        subject: String,
        predicate: String,
        object: String,
    ) -> Result<Relationship, NervusError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.worker
            .tx
            .send(Command::AddFact {
                subject,
                predicate,
                object,
                reply: reply_tx,
            })
            .map_err(|_| NervusError::Other {
                message: "worker thread is not available".to_string(),
            })?;
        reply_rx.recv().map_err(|_| NervusError::Other {
            message: "worker thread is not available".to_string(),
        })?
    }

    pub fn close(&self) {
        self.worker.close();
    }
}

pub struct Statement {
    worker: Arc<WorkerHandle>,
    statement_id: u64,
    column_names: Vec<String>,
    buffered_rows: Mutex<VecDeque<Vec<WireValue>>>,
    current_row: Mutex<Option<Vec<WireValue>>>,
    done: Mutex<bool>,
    finalized: Mutex<bool>,
}

impl Statement {
    fn new(worker: Arc<WorkerHandle>, statement_id: u64, column_names: Vec<String>) -> Self {
        Self {
            worker,
            statement_id,
            column_names,
            buffered_rows: Mutex::new(VecDeque::new()),
            current_row: Mutex::new(None),
            done: Mutex::new(false),
            finalized: Mutex::new(false),
        }
    }

    fn ensure_not_finalized(&self) -> Result<(), NervusError> {
        if *self.finalized.lock().map_err(|_| NervusError::Other {
            message: "mutex poisoned".to_string(),
        })? {
            return Err(NervusError::InvalidArgument {
                message: "statement already finalized".to_string(),
            });
        }
        Ok(())
    }

    fn cell(&self, index: u32) -> Result<Option<WireValue>, NervusError> {
        self.ensure_not_finalized()?;
        let guard = self.current_row.lock().map_err(|_| NervusError::Other {
            message: "mutex poisoned".to_string(),
        })?;
        let idx = usize::try_from(index).unwrap_or(usize::MAX);
        Ok(guard.as_ref().and_then(|row| row.get(idx)).cloned())
    }
}

impl Statement {
    pub fn step(&self) -> Result<bool, NervusError> {
        self.ensure_not_finalized()?;
        {
            let mut current = self.current_row.lock().map_err(|_| NervusError::Other {
                message: "mutex poisoned".to_string(),
            })?;
            *current = None;
        }

        let mut buffered = self.buffered_rows.lock().map_err(|_| NervusError::Other {
            message: "mutex poisoned".to_string(),
        })?;

        if buffered.is_empty() {
            let done = *self.done.lock().map_err(|_| NervusError::Other {
                message: "mutex poisoned".to_string(),
            })?;
            if done {
                return Ok(false);
            }
        }

        if buffered.is_empty() {
            let (reply_tx, reply_rx) = mpsc::channel();
            self.worker
                .tx
                .send(Command::NextBatch {
                    statement_id: self.statement_id,
                    max_rows: 256,
                    reply: reply_tx,
                })
                .map_err(|_| NervusError::Other {
                    message: "worker thread is not available".to_string(),
                })?;
            let batch = reply_rx.recv().map_err(|_| NervusError::Other {
                message: "worker thread is not available".to_string(),
            })??;

            for row in batch.rows {
                buffered.push_back(row);
            }

            if batch.done && buffered.is_empty() {
                let mut done = self.done.lock().map_err(|_| NervusError::Other {
                    message: "mutex poisoned".to_string(),
                })?;
                *done = true;
                return Ok(false);
            }
        }

        let Some(row) = buffered.pop_front() else {
            return Ok(false);
        };

        let mut current = self.current_row.lock().map_err(|_| NervusError::Other {
            message: "mutex poisoned".to_string(),
        })?;
        *current = Some(row);
        Ok(true)
    }

    pub fn column_count(&self) -> Result<u32, NervusError> {
        self.ensure_not_finalized()?;
        Ok(u32::try_from(self.column_names.len()).unwrap_or(u32::MAX))
    }

    pub fn column_name(&self, index: u32) -> Result<Option<String>, NervusError> {
        self.ensure_not_finalized()?;
        let idx = usize::try_from(index).ok().unwrap_or(usize::MAX);
        Ok(self.column_names.get(idx).cloned())
    }

    pub fn column_type(&self, index: u32) -> Result<ValueType, NervusError> {
        Ok(match self.cell(index)? {
            Some(v) => v.value_type(),
            None => ValueType::Null,
        })
    }

    pub fn column_text(&self, index: u32) -> Result<Option<String>, NervusError> {
        match self.cell(index)? {
            Some(WireValue::Text(s)) => Ok(Some(s)),
            _ => Ok(None),
        }
    }

    pub fn column_double(&self, index: u32) -> Result<Option<f64>, NervusError> {
        match self.cell(index)? {
            Some(WireValue::Float(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    pub fn column_bool(&self, index: u32) -> Result<Option<bool>, NervusError> {
        match self.cell(index)? {
            Some(WireValue::Bool(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    pub fn column_node_id(&self, index: u32) -> Result<Option<u64>, NervusError> {
        match self.cell(index)? {
            Some(WireValue::Node(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    pub fn column_relationship(&self, index: u32) -> Result<Option<Relationship>, NervusError> {
        match self.cell(index)? {
            Some(WireValue::Relationship(v)) => Ok(Some(v)),
            _ => Ok(None),
        }
    }

    pub fn finalize(&self) {
        let mut finalized = match self.finalized.lock() {
            Ok(v) => v,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *finalized {
            return;
        }
        *finalized = true;
        let _ = self.worker.tx.send(Command::Finalize {
            statement_id: self.statement_id,
        });
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        self.finalize();
    }
}

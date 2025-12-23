//! C FFI bindings for NervusDB.
//!
//! Safety requirements are documented in the C header file: `include/nervusdb.h`
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::ptr;

use crate::{Database, Error, Options, QueryCriteria, Triple};

#[allow(non_camel_case_types)]
pub type nervusdb_status = i32;

pub const NERVUSDB_OK: nervusdb_status = 0;
pub const NERVUSDB_ERR_INVALID_ARGUMENT: nervusdb_status = 1;
pub const NERVUSDB_ERR_OPEN: nervusdb_status = 2;
pub const NERVUSDB_ERR_INTERNAL: nervusdb_status = 3;
pub const NERVUSDB_ERR_CALLBACK: nervusdb_status = 4;
pub const NERVUSDB_ROW: nervusdb_status = 100;
pub const NERVUSDB_DONE: nervusdb_status = 101;

pub const NERVUSDB_ABI_VERSION: u32 = 1;

#[allow(non_camel_case_types)]
pub type nervusdb_value_type = i32;

pub const NERVUSDB_VALUE_NULL: nervusdb_value_type = 0;
pub const NERVUSDB_VALUE_TEXT: nervusdb_value_type = 1;
pub const NERVUSDB_VALUE_FLOAT: nervusdb_value_type = 2;
pub const NERVUSDB_VALUE_BOOL: nervusdb_value_type = 3;
pub const NERVUSDB_VALUE_NODE: nervusdb_value_type = 4;
pub const NERVUSDB_VALUE_RELATIONSHIP: nervusdb_value_type = 5;

#[repr(C)]
pub struct nervusdb_db {
    _private: [u8; 0],
}

#[repr(C)]
pub struct nervusdb_stmt {
    _private: [u8; 0],
}

#[repr(C)]
pub struct nervusdb_error {
    pub code: nervusdb_status,
    pub message: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct nervusdb_relationship {
    pub subject_id: u64,
    pub predicate_id: u64,
    pub object_id: u64,
}

#[repr(C)]
pub struct nervusdb_query_criteria {
    pub subject_id: u64,
    pub predicate_id: u64,
    pub object_id: u64,
    pub has_subject: bool,
    pub has_predicate: bool,
    pub has_object: bool,
}

#[allow(non_camel_case_types)]
pub type nervusdb_triple_callback = Option<extern "C" fn(u64, u64, u64, *mut c_void) -> bool>;

static NERVUSDB_VERSION_STR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

#[inline]
fn clear_error(out_error: *mut *mut nervusdb_error) {
    if out_error.is_null() {
        return;
    }
    unsafe {
        *out_error = ptr::null_mut();
    }
}

fn set_error(out_error: *mut *mut nervusdb_error, code: nervusdb_status, message: &str) {
    if out_error.is_null() {
        return;
    }

    let c_message =
        CString::new(message).unwrap_or_else(|_| CString::new("invalid error message").unwrap());
    let error = Box::new(nervusdb_error {
        code,
        message: c_message.into_raw(),
    });
    unsafe {
        *out_error = Box::into_raw(error);
    }
}

fn status_from_error(err: &Error) -> nervusdb_status {
    match err {
        Error::NotImplemented(_) => NERVUSDB_ERR_INVALID_ARGUMENT,
        Error::InvalidCursor(_) | Error::NotFound | Error::Other(_) => NERVUSDB_ERR_INTERNAL,
        _ => NERVUSDB_ERR_INTERNAL,
    }
}

fn db_from_ptr<'a>(
    db: *mut nervusdb_db,
    out_error: *mut *mut nervusdb_error,
) -> Result<&'a mut Database, nervusdb_status> {
    if db.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "database pointer is null",
        );
        return Err(NERVUSDB_ERR_INVALID_ARGUMENT);
    }
    unsafe {
        let handle = &mut *(db as *mut DatabaseHandle);
        // SAFETY: We need mutable access to Database for write operations.
        // Arc::get_mut would fail if there are active statements, so we use
        // Arc::as_ptr and cast to mut. Caller must ensure no concurrent access.
        Ok(&mut *(Arc::as_ptr(&handle.db) as *mut Database))
    }
}

fn db_arc_from_ptr(
    db: *mut nervusdb_db,
    out_error: *mut *mut nervusdb_error,
) -> Result<Arc<Database>, nervusdb_status> {
    if db.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "database pointer is null",
        );
        return Err(NERVUSDB_ERR_INVALID_ARGUMENT);
    }
    unsafe {
        let handle = &*(db as *mut DatabaseHandle);
        Ok(Arc::clone(&handle.db))
    }
}

fn cstr_to_owned(
    value: *const c_char,
    out_error: *mut *mut nervusdb_error,
    name: &str,
) -> Result<String, nervusdb_status> {
    if value.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            &format!("{name} pointer is null"),
        );
        return Err(NERVUSDB_ERR_INVALID_ARGUMENT);
    }
    match unsafe { CStr::from_ptr(value) }.to_str() {
        Ok(v) => Ok(v.to_owned()),
        Err(_) => {
            set_error(
                out_error,
                NERVUSDB_ERR_INVALID_ARGUMENT,
                &format!("{name} is not valid UTF-8"),
            );
            Err(NERVUSDB_ERR_INVALID_ARGUMENT)
        }
    }
}

fn criteria_from_ffi(ffi: &nervusdb_query_criteria) -> QueryCriteria {
    QueryCriteria {
        subject_id: if ffi.has_subject {
            Some(ffi.subject_id)
        } else {
            None
        },
        predicate_id: if ffi.has_predicate {
            Some(ffi.predicate_id)
        } else {
            None
        },
        object_id: if ffi.has_object {
            Some(ffi.object_id)
        } else {
            None
        },
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_abi_version() -> u32 {
    NERVUSDB_ABI_VERSION
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_version() -> *const c_char {
    NERVUSDB_VERSION_STR.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_free_string(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(value));
    }
}

use std::sync::Arc;

/// Internal wrapper for Arc<Database> used by FFI
struct DatabaseHandle {
    db: Arc<Database>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_open(
    path: *const c_char,
    out_db: *mut *mut nervusdb_db,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    if path.is_null() || out_db.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "path/out_db pointer is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    let path_str = match cstr_to_owned(path, out_error, "path") {
        Ok(v) => v,
        Err(code) => return code,
    };

    match Database::open(Options::new(path_str.as_str())) {
        Ok(db) => {
            // Note: Arc is used for shared ownership with statements, not thread safety.
            // FFI calls are single-threaded.
            #[allow(clippy::arc_with_non_send_sync)]
            let handle = Box::new(DatabaseHandle { db: Arc::new(db) });
            unsafe {
                *out_db = Box::into_raw(handle) as *mut nervusdb_db;
            }
            NERVUSDB_OK
        }
        Err(err) => {
            set_error(out_error, NERVUSDB_ERR_OPEN, &err.to_string());
            NERVUSDB_ERR_OPEN
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_close(db: *mut nervusdb_db) {
    if db.is_null() {
        return;
    }
    let handle = db as *mut DatabaseHandle;
    unsafe {
        drop(Box::from_raw(handle));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_intern(
    db: *mut nervusdb_db,
    value: *const c_char,
    out_id: *mut u64,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    if value.is_null() || out_id.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "value/out_id pointer is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let value_str = match cstr_to_owned(value, out_error, "value") {
        Ok(v) => v,
        Err(code) => return code,
    };

    match db.intern(value_str.as_str()) {
        Ok(id) => {
            unsafe {
                *out_id = id;
            }
            NERVUSDB_OK
        }
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_resolve_id(
    db: *mut nervusdb_db,
    value: *const c_char,
    out_id: *mut u64,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    if value.is_null() || out_id.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "value/out_id pointer is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let value_str = match cstr_to_owned(value, out_error, "value") {
        Ok(v) => v,
        Err(code) => return code,
    };

    match db.resolve_id(value_str.as_str()) {
        Ok(Some(id)) => {
            unsafe {
                *out_id = id;
            }
            NERVUSDB_OK
        }
        Ok(None) => {
            unsafe {
                *out_id = 0;
            }
            NERVUSDB_OK
        }
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_resolve_str(
    db: *mut nervusdb_db,
    id: u64,
    out_value: *mut *mut c_char,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    if out_value.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "out_value pointer is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    unsafe {
        *out_value = ptr::null_mut();
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    match db.resolve_str(id) {
        Ok(Some(value)) => match CString::new(value) {
            Ok(c_value) => {
                unsafe {
                    *out_value = c_value.into_raw();
                }
                NERVUSDB_OK
            }
            Err(_) => {
                set_error(out_error, NERVUSDB_ERR_INTERNAL, "value contained NUL byte");
                NERVUSDB_ERR_INTERNAL
            }
        },
        Ok(None) => NERVUSDB_OK,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_add_triple(
    db: *mut nervusdb_db,
    subject_id: u64,
    predicate_id: u64,
    object_id: u64,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let triple = Triple::new(subject_id, predicate_id, object_id);
    let insert_result = if let Some(txn) = db.active_write.as_mut() {
        crate::storage::disk::insert_triple(txn, &triple).map(|_| ())
    } else {
        db.store.insert(&triple).map(|_| ())
    };
    match insert_result {
        Ok(()) => NERVUSDB_OK,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_begin_transaction(
    db: *mut nervusdb_db,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };
    match db.begin_transaction() {
        Ok(()) => NERVUSDB_OK,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_commit_transaction(
    db: *mut nervusdb_db,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };
    match db.commit_transaction() {
        Ok(()) => NERVUSDB_OK,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_abort_transaction(
    db: *mut nervusdb_db,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };
    match db.abort_transaction() {
        Ok(()) => NERVUSDB_OK,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            code
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_query_triples(
    db: *mut nervusdb_db,
    criteria: *const nervusdb_query_criteria,
    callback: nervusdb_triple_callback,
    user_data: *mut c_void,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    let callback = match callback {
        Some(cb) => cb,
        None => {
            set_error(out_error, NERVUSDB_ERR_INVALID_ARGUMENT, "callback is null");
            return NERVUSDB_ERR_INVALID_ARGUMENT;
        }
    };

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let query = if criteria.is_null() {
        QueryCriteria::default()
    } else {
        unsafe { criteria_from_ffi(&*criteria) }
    };

    for triple in db.query(query) {
        let should_continue = callback(
            triple.subject_id,
            triple.predicate_id,
            triple.object_id,
            user_data,
        );
        if !should_continue {
            break;
        }
    }

    NERVUSDB_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_exec_cypher(
    db: *mut nervusdb_db,
    query: *const c_char,
    params_json: *const c_char,
    out_json: *mut *mut c_char,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    if query.is_null() || out_json.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "query/out_json pointer is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    unsafe {
        *out_json = ptr::null_mut();
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let query_str = match cstr_to_owned(query, out_error, "query") {
        Ok(v) => v,
        Err(code) => return code,
    };

    let params: Option<HashMap<String, serde_json::Value>> = if params_json.is_null() {
        None
    } else {
        let raw = match cstr_to_owned(params_json, out_error, "params_json") {
            Ok(v) => v,
            Err(code) => return code,
        };
        if raw.trim().is_empty() {
            None
        } else {
            match serde_json::from_str::<HashMap<String, serde_json::Value>>(&raw) {
                Ok(map) => Some(map),
                Err(_) => {
                    set_error(
                        out_error,
                        NERVUSDB_ERR_INVALID_ARGUMENT,
                        "params_json must be a JSON object",
                    );
                    return NERVUSDB_ERR_INVALID_ARGUMENT;
                }
            }
        }
    };

    let results = match db.execute_query_with_params(query_str.as_str(), params) {
        Ok(r) => r,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            return code;
        }
    };

    let json_results: Vec<HashMap<String, serde_json::Value>> = results
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(k, v)| {
                    let json_val = match v {
                        crate::query::executor::Value::String(s) => serde_json::Value::String(s),
                        crate::query::executor::Value::Float(f) => serde_json::json!(f),
                        crate::query::executor::Value::Boolean(b) => serde_json::Value::Bool(b),
                        crate::query::executor::Value::Null => serde_json::Value::Null,
                        crate::query::executor::Value::Node(id) => serde_json::json!({ "id": id }),
                        crate::query::executor::Value::Relationship(id) => {
                            serde_json::json!({ "id": id })
                        }
                    };
                    (k, json_val)
                })
                .collect()
        })
        .collect();

    let json_string = match serde_json::to_string(&json_results) {
        Ok(s) => s,
        Err(_) => {
            set_error(
                out_error,
                NERVUSDB_ERR_INTERNAL,
                "failed to serialize results to JSON",
            );
            return NERVUSDB_ERR_INTERNAL;
        }
    };

    let c_json = match CString::new(json_string) {
        Ok(s) => s,
        Err(_) => {
            set_error(out_error, NERVUSDB_ERR_INTERNAL, "JSON contained NUL byte");
            return NERVUSDB_ERR_INTERNAL;
        }
    };

    unsafe {
        *out_json = c_json.into_raw();
    }

    NERVUSDB_OK
}

// ---------------------------------------------------------------------------
// Statement API (SQLite-style row iterator) - TRUE STREAMING
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum StmtValue {
    Null,
    Text(Box<[u8]>), // NUL-terminated; may contain embedded NULs (use column_bytes)
    Float(f64),
    Boolean(bool),
    Node(u64),
    Relationship(Triple),
}

/// True streaming statement - uses Arc<Database> for 'static iterator
struct CypherStatement {
    columns: Vec<CString>,
    column_names: Vec<String>,
    /// The streaming iterator - truly lazy, no collect()
    iterator:
        Option<Box<dyn Iterator<Item = Result<crate::query::executor::Record, Error>> + 'static>>,
    current_row: Option<Vec<StmtValue>>,
}

fn stmt_from_ptr<'a>(
    stmt: *mut nervusdb_stmt,
    out_error: *mut *mut nervusdb_error,
) -> Result<&'a mut CypherStatement, nervusdb_status> {
    if stmt.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "statement pointer is null",
        );
        return Err(NERVUSDB_ERR_INVALID_ARGUMENT);
    }
    unsafe { Ok(&mut *(stmt as *mut CypherStatement)) }
}

fn parse_params_json(
    params_json: *const c_char,
    out_error: *mut *mut nervusdb_error,
) -> Result<Option<HashMap<String, serde_json::Value>>, nervusdb_status> {
    if params_json.is_null() {
        return Ok(None);
    }
    let raw = cstr_to_owned(params_json, out_error, "params_json")?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<HashMap<String, serde_json::Value>>(&raw)
        .map(Some)
        .map_err(|_| {
            set_error(
                out_error,
                NERVUSDB_ERR_INVALID_ARGUMENT,
                "params_json must be a JSON object",
            );
            NERVUSDB_ERR_INVALID_ARGUMENT
        })
}

fn infer_projection_alias(expr: &crate::query::ast::Expression) -> String {
    use crate::query::ast::Expression;
    match expr {
        Expression::Variable(name) => name.clone(),
        Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
        _ => "col".to_string(),
    }
}

fn convert_stmt_value(value: crate::query::executor::Value) -> StmtValue {
    match value {
        crate::query::executor::Value::String(s) => {
            let mut bytes = s.into_bytes();
            bytes.push(0);
            StmtValue::Text(bytes.into_boxed_slice())
        }
        crate::query::executor::Value::Float(f) => StmtValue::Float(f),
        crate::query::executor::Value::Boolean(b) => StmtValue::Boolean(b),
        crate::query::executor::Value::Null => StmtValue::Null,
        crate::query::executor::Value::Node(id) => StmtValue::Node(id),
        crate::query::executor::Value::Relationship(triple) => StmtValue::Relationship(triple),
    }
}

fn stmt_cell(stmt: &CypherStatement, column: i32) -> Option<&StmtValue> {
    let idx = usize::try_from(column).ok()?;
    stmt.current_row.as_ref()?.get(idx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_prepare_v2(
    db: *mut nervusdb_db,
    query: *const c_char,
    params_json: *const c_char,
    out_stmt: *mut *mut nervusdb_stmt,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    if query.is_null() || out_stmt.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "query/out_stmt pointer is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }
    unsafe {
        *out_stmt = ptr::null_mut();
    }

    // Get Arc<Database> for streaming execution
    let db_arc = match db_arc_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let query_str = match cstr_to_owned(query, out_error, "query") {
        Ok(v) => v,
        Err(code) => return code,
    };

    let params = match parse_params_json(params_json, out_error) {
        Ok(v) => v,
        Err(code) => return code,
    };

    // Parse query and extract column names from RETURN clause
    let ast = match crate::query::parser::Parser::parse(query_str.as_str()) {
        Ok(ast) => ast,
        Err(err) => {
            set_error(out_error, NERVUSDB_ERR_INVALID_ARGUMENT, &err.to_string());
            return NERVUSDB_ERR_INVALID_ARGUMENT;
        }
    };

    let mut projection_names: Vec<String> = Vec::new();
    for clause in &ast.clauses {
        if let crate::query::ast::Clause::Return(r) = clause {
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

    // Check for duplicate column names
    if !projection_names.is_empty() {
        let mut seen = std::collections::HashSet::new();
        for name in &projection_names {
            if !seen.insert(name) {
                set_error(
                    out_error,
                    NERVUSDB_ERR_INVALID_ARGUMENT,
                    &format!("duplicate column name: {name}; use explicit aliases"),
                );
                return NERVUSDB_ERR_INVALID_ARGUMENT;
            }
        }
    }

    // Generate execution plan
    let planner = crate::query::planner::QueryPlanner::new();
    let plan = match planner.plan(ast) {
        Ok(p) => p,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            return code;
        }
    };

    // Convert params
    let param_values: HashMap<String, crate::query::executor::Value> = params
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, Database::serde_value_to_executor_value(v)))
        .collect();

    // Build CString columns
    let mut c_columns = Vec::with_capacity(projection_names.len());
    for name in &projection_names {
        match CString::new(name.as_str()) {
            Ok(c) => c_columns.push(c),
            Err(_) => {
                set_error(
                    out_error,
                    NERVUSDB_ERR_INTERNAL,
                    "column name contained NUL byte",
                );
                return NERVUSDB_ERR_INTERNAL;
            }
        }
    }

    // Create Arc-based execution context for true streaming
    // Note: Arc is used for shared ownership, not thread safety. FFI calls are single-threaded.
    #[allow(clippy::arc_with_non_send_sync)]
    let ctx = Arc::new(crate::query::executor::ArcExecutionContext::new(
        db_arc,
        param_values,
    ));

    // Execute with streaming - NO collect()!
    let iterator = match plan.execute_streaming(ctx) {
        Ok(iter) => iter,
        Err(err) => {
            let message = err.to_string();
            let code = status_from_error(&err);
            set_error(out_error, code, &message);
            return code;
        }
    };

    let stmt = Box::new(CypherStatement {
        columns: c_columns,
        column_names: projection_names,
        iterator: Some(iterator),
        current_row: None,
    });

    unsafe {
        *out_stmt = Box::into_raw(stmt) as *mut nervusdb_stmt;
    }

    NERVUSDB_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_step(
    stmt: *mut nervusdb_stmt,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);
    let stmt = match stmt_from_ptr(stmt, out_error) {
        Ok(v) => v,
        Err(code) => return code,
    };

    stmt.current_row = None;

    // Get next row from iterator (lazy execution happens inside)
    if let Some(ref mut iter) = stmt.iterator {
        match iter.next() {
            Some(Ok(record)) => {
                // Convert record to StmtValue row
                let mut row = Vec::with_capacity(stmt.column_names.len());
                for col in &stmt.column_names {
                    let value = record
                        .values
                        .get(col)
                        .cloned()
                        .unwrap_or(crate::query::executor::Value::Null);
                    row.push(convert_stmt_value(value));
                }
                stmt.current_row = Some(row);
                return NERVUSDB_ROW;
            }
            Some(Err(err)) => {
                let message = err.to_string();
                let code = status_from_error(&err);
                set_error(out_error, code, &message);
                return code;
            }
            None => {
                return NERVUSDB_DONE;
            }
        }
    }

    NERVUSDB_DONE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_count(stmt: *mut nervusdb_stmt) -> i32 {
    if stmt.is_null() {
        return 0;
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    i32::try_from(stmt.columns.len()).unwrap_or(i32::MAX)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_name(
    stmt: *mut nervusdb_stmt,
    column: i32,
) -> *const c_char {
    if stmt.is_null() {
        return ptr::null();
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    let idx = match usize::try_from(column) {
        Ok(v) => v,
        Err(_) => return ptr::null(),
    };
    stmt.columns
        .get(idx)
        .map(|s| s.as_ptr())
        .unwrap_or(ptr::null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_type(
    stmt: *mut nervusdb_stmt,
    column: i32,
) -> nervusdb_value_type {
    if stmt.is_null() {
        return NERVUSDB_VALUE_NULL;
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Null) => NERVUSDB_VALUE_NULL,
        Some(StmtValue::Text(_)) => NERVUSDB_VALUE_TEXT,
        Some(StmtValue::Float(_)) => NERVUSDB_VALUE_FLOAT,
        Some(StmtValue::Boolean(_)) => NERVUSDB_VALUE_BOOL,
        Some(StmtValue::Node(_)) => NERVUSDB_VALUE_NODE,
        Some(StmtValue::Relationship(_)) => NERVUSDB_VALUE_RELATIONSHIP,
        None => NERVUSDB_VALUE_NULL,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_text(
    stmt: *mut nervusdb_stmt,
    column: i32,
) -> *const c_char {
    if stmt.is_null() {
        return ptr::null();
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Text(bytes)) => bytes.as_ptr() as *const c_char,
        _ => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_bytes(stmt: *mut nervusdb_stmt, column: i32) -> i32 {
    if stmt.is_null() {
        return 0;
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Text(bytes)) => {
            i32::try_from(bytes.len().saturating_sub(1)).unwrap_or(i32::MAX)
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_double(stmt: *mut nervusdb_stmt, column: i32) -> f64 {
    if stmt.is_null() {
        return 0.0;
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Float(v)) => *v,
        _ => 0.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_bool(stmt: *mut nervusdb_stmt, column: i32) -> i32 {
    if stmt.is_null() {
        return 0;
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Boolean(v)) => i32::from(*v),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_node_id(stmt: *mut nervusdb_stmt, column: i32) -> u64 {
    if stmt.is_null() {
        return 0;
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Node(id)) => *id,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_column_relationship(
    stmt: *mut nervusdb_stmt,
    column: i32,
) -> nervusdb_relationship {
    if stmt.is_null() {
        return nervusdb_relationship::default();
    }
    let stmt = unsafe { &*(stmt as *mut CypherStatement) };
    match stmt_cell(stmt, column) {
        Some(StmtValue::Relationship(triple)) => nervusdb_relationship {
            subject_id: triple.subject_id,
            predicate_id: triple.predicate_id,
            object_id: triple.object_id,
        },
        _ => nervusdb_relationship::default(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_finalize(stmt: *mut nervusdb_stmt) {
    if stmt.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(stmt as *mut CypherStatement));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_free_error(err: *mut nervusdb_error) {
    if err.is_null() {
        return;
    }
    let boxed = unsafe { Box::from_raw(err) };
    if !boxed.message.is_null() {
        unsafe {
            drop(CString::from_raw(boxed.message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    static CALLBACK_COUNT: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn collect(_s: u64, _p: u64, _o: u64, _data: *mut c_void) -> bool {
        CALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
        true
    }

    #[test]
    fn ffi_roundtrip() {
        unsafe {
            CALLBACK_COUNT.store(0, Ordering::SeqCst);
            let dir = tempdir().unwrap();
            let path = dir.path().join("ffi_roundtrip");
            let path_c = CString::new(path.to_string_lossy().as_bytes()).unwrap();

            let mut db_ptr: *mut nervusdb_db = ptr::null_mut();
            let mut err_ptr: *mut nervusdb_error = ptr::null_mut();
            let status = nervusdb_open(path_c.as_ptr(), &mut db_ptr, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(!db_ptr.is_null());
            assert!(err_ptr.is_null());

            let mut id = 0u64;
            let name = CString::new("Alice").unwrap();
            let status = nervusdb_intern(db_ptr, name.as_ptr(), &mut id, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(id > 0);
            assert!(err_ptr.is_null());

            let mut resolved_id = 0u64;
            let status = nervusdb_resolve_id(db_ptr, name.as_ptr(), &mut resolved_id, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());
            assert_eq!(resolved_id, id);

            let mut resolved_str: *mut c_char = ptr::null_mut();
            let status = nervusdb_resolve_str(db_ptr, id, &mut resolved_str, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());
            assert!(!resolved_str.is_null());
            let roundtrip = CStr::from_ptr(resolved_str).to_string_lossy().to_string();
            assert_eq!(roundtrip, "Alice");
            nervusdb_free_string(resolved_str);

            let status = nervusdb_add_triple(db_ptr, id, id, id, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());

            let query_status = nervusdb_query_triples(
                db_ptr,
                ptr::null(),
                Some(collect),
                ptr::null_mut(),
                &mut err_ptr,
            );
            assert_eq!(query_status, NERVUSDB_OK);
            assert!(CALLBACK_COUNT.load(Ordering::SeqCst) >= 1);
            assert!(err_ptr.is_null());

            // Transaction API + exec_cypher smoke
            let status = nervusdb_begin_transaction(db_ptr, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());

            let alice = intern(db_ptr, "alice", &mut err_ptr);
            let name_id = intern(db_ptr, "name", &mut err_ptr);
            let alice_val = intern(db_ptr, "Alice", &mut err_ptr);
            let age_id = intern(db_ptr, "age", &mut err_ptr);
            let age_val = intern(db_ptr, "30", &mut err_ptr);
            let bob = intern(db_ptr, "bob", &mut err_ptr);
            let bob_val = intern(db_ptr, "Bob", &mut err_ptr);
            let knows_id = intern(db_ptr, "knows", &mut err_ptr);

            assert!(err_ptr.is_null());

            assert_eq!(
                nervusdb_add_triple(db_ptr, alice, name_id, alice_val, &mut err_ptr),
                NERVUSDB_OK
            );
            assert_eq!(
                nervusdb_add_triple(db_ptr, alice, age_id, age_val, &mut err_ptr),
                NERVUSDB_OK
            );
            assert_eq!(
                nervusdb_add_triple(db_ptr, bob, name_id, bob_val, &mut err_ptr),
                NERVUSDB_OK
            );
            assert_eq!(
                nervusdb_add_triple(db_ptr, alice, knows_id, bob, &mut err_ptr),
                NERVUSDB_OK
            );
            assert!(err_ptr.is_null());

            let status = nervusdb_commit_transaction(db_ptr, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());

            let query = CString::new("MATCH (n) RETURN n").unwrap();
            let mut out_json: *mut c_char = ptr::null_mut();
            let status = nervusdb_exec_cypher(
                db_ptr,
                query.as_ptr(),
                ptr::null(),
                &mut out_json,
                &mut err_ptr,
            );
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());
            assert!(!out_json.is_null());
            let json = CStr::from_ptr(out_json).to_string_lossy().to_string();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert!(parsed.is_array());
            nervusdb_free_string(out_json);

            // Statement API smoke
            let query = CString::new("MATCH (a)-[r:knows]->(b) RETURN a, r, b").unwrap();
            let mut stmt: *mut nervusdb_stmt = ptr::null_mut();
            let status =
                nervusdb_prepare_v2(db_ptr, query.as_ptr(), ptr::null(), &mut stmt, &mut err_ptr);
            assert_eq!(status, NERVUSDB_OK);
            assert!(err_ptr.is_null());
            assert!(!stmt.is_null());

            assert_eq!(nervusdb_column_count(stmt), 3);
            assert_eq!(
                CStr::from_ptr(nervusdb_column_name(stmt, 0)).to_string_lossy(),
                "a"
            );
            assert_eq!(
                CStr::from_ptr(nervusdb_column_name(stmt, 1)).to_string_lossy(),
                "r"
            );
            assert_eq!(
                CStr::from_ptr(nervusdb_column_name(stmt, 2)).to_string_lossy(),
                "b"
            );

            let step = nervusdb_step(stmt, &mut err_ptr);
            assert_eq!(step, NERVUSDB_ROW);
            assert!(err_ptr.is_null());

            assert_eq!(nervusdb_column_type(stmt, 0), NERVUSDB_VALUE_NODE);
            assert_eq!(nervusdb_column_type(stmt, 1), NERVUSDB_VALUE_RELATIONSHIP);
            assert_eq!(nervusdb_column_type(stmt, 2), NERVUSDB_VALUE_NODE);

            let a_id = nervusdb_column_node_id(stmt, 0);
            let b_id = nervusdb_column_node_id(stmt, 2);
            assert_eq!(a_id, alice);
            assert_eq!(b_id, bob);

            let rel = nervusdb_column_relationship(stmt, 1);
            assert_eq!(rel.subject_id, alice);
            assert_eq!(rel.predicate_id, knows_id);
            assert_eq!(rel.object_id, bob);

            let step = nervusdb_step(stmt, &mut err_ptr);
            assert_eq!(step, NERVUSDB_DONE);
            assert!(err_ptr.is_null());

            nervusdb_finalize(stmt);

            nervusdb_close(db_ptr);
            if !err_ptr.is_null() {
                nervusdb_free_error(err_ptr);
            }
        }
    }

    fn intern(db: *mut nervusdb_db, value: &str, err: &mut *mut nervusdb_error) -> u64 {
        let c_value = CString::new(value).unwrap();
        let mut out = 0u64;
        let status = unsafe { nervusdb_intern(db, c_value.as_ptr(), &mut out, err) };
        assert_eq!(status, NERVUSDB_OK);
        out
    }
}

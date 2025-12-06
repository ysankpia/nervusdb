use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::ptr;

use crate::algorithms::{PageRankOptions, bfs_shortest_path, dijkstra_shortest_path, pagerank};
use crate::{Database, Error, Options, QueryCriteria, Triple};

#[allow(non_camel_case_types)]
pub type nervusdb_status = i32;

pub const NERVUSDB_OK: nervusdb_status = 0;
pub const NERVUSDB_ERR_INVALID_ARGUMENT: nervusdb_status = 1;
pub const NERVUSDB_ERR_OPEN: nervusdb_status = 2;
pub const NERVUSDB_ERR_INTERNAL: nervusdb_status = 3;
pub const NERVUSDB_ERR_CALLBACK: nervusdb_status = 4;

#[repr(C)]
pub struct nervusdb_db {
    _private: [u8; 0],
}

#[repr(C)]
pub struct nervusdb_error {
    pub code: nervusdb_status,
    pub message: *mut c_char,
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
    unsafe { Ok(&mut *(db as *mut Database)) }
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

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(value) => value,
        Err(_) => {
            set_error(
                out_error,
                NERVUSDB_ERR_INVALID_ARGUMENT,
                "path is not valid UTF-8",
            );
            return NERVUSDB_ERR_INVALID_ARGUMENT;
        }
    };

    match Database::open(Options::new(path_str)) {
        Ok(db) => {
            let boxed: Box<Database> = Box::new(db);
            unsafe {
                *out_db = Box::into_raw(boxed) as *mut nervusdb_db;
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
    let db_ptr = db as *mut Database;
    unsafe {
        drop(Box::from_raw(db_ptr));
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

    let value_str = match unsafe { CStr::from_ptr(value) }.to_str() {
        Ok(v) => v,
        Err(_) => {
            set_error(
                out_error,
                NERVUSDB_ERR_INVALID_ARGUMENT,
                "value is not valid UTF-8",
            );
            return NERVUSDB_ERR_INVALID_ARGUMENT;
        }
    };

    match db.intern(value_str) {
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
    match db.batch_insert(std::slice::from_ref(&triple)) {
        Ok(_) => NERVUSDB_OK,
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

// ============================================================================
// Graph Algorithms FFI
// ============================================================================

/// Result structure for BFS shortest path
#[repr(C)]
pub struct nervusdb_path_result {
    /// Array of node IDs in the path
    pub path: *mut u64,
    /// Number of nodes in the path
    pub path_len: usize,
    /// Total cost/distance
    pub cost: f64,
    /// Number of hops
    pub hops: usize,
}

/// Find shortest path using BFS (unweighted)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_bfs_shortest_path(
    db: *mut nervusdb_db,
    start_id: u64,
    end_id: u64,
    predicate_id: u64,
    has_predicate: bool,
    max_hops: usize,
    bidirectional: bool,
    out_result: *mut nervusdb_path_result,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);

    if out_result.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "out_result is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let pred = if has_predicate {
        Some(predicate_id)
    } else {
        None
    };

    match bfs_shortest_path(
        db.get_store(),
        start_id,
        end_id,
        pred,
        max_hops,
        bidirectional,
    ) {
        Some(result) => {
            let mut path_vec = result.path.into_boxed_slice();
            let path_ptr = path_vec.as_mut_ptr();
            let path_len = path_vec.len();
            std::mem::forget(path_vec);

            unsafe {
                (*out_result).path = path_ptr;
                (*out_result).path_len = path_len;
                (*out_result).cost = result.cost;
                (*out_result).hops = result.hops;
            }
            NERVUSDB_OK
        }
        None => {
            unsafe {
                (*out_result).path = ptr::null_mut();
                (*out_result).path_len = 0;
                (*out_result).cost = 0.0;
                (*out_result).hops = 0;
            }
            NERVUSDB_OK // No path found is not an error
        }
    }
}

/// Find shortest path using Dijkstra (weighted, uniform weight = 1.0)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_dijkstra_shortest_path(
    db: *mut nervusdb_db,
    start_id: u64,
    end_id: u64,
    predicate_id: u64,
    has_predicate: bool,
    max_hops: usize,
    out_result: *mut nervusdb_path_result,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);

    if out_result.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "out_result is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let pred = if has_predicate {
        Some(predicate_id)
    } else {
        None
    };

    // Use uniform weight of 1.0
    let weight_fn = |_s: u64, _p: u64, _o: u64| 1.0;

    match dijkstra_shortest_path(db.get_store(), start_id, end_id, pred, weight_fn, max_hops) {
        Some(result) => {
            let mut path_vec = result.path.into_boxed_slice();
            let path_ptr = path_vec.as_mut_ptr();
            let path_len = path_vec.len();
            std::mem::forget(path_vec);

            unsafe {
                (*out_result).path = path_ptr;
                (*out_result).path_len = path_len;
                (*out_result).cost = result.cost;
                (*out_result).hops = result.hops;
            }
            NERVUSDB_OK
        }
        None => {
            unsafe {
                (*out_result).path = ptr::null_mut();
                (*out_result).path_len = 0;
                (*out_result).cost = 0.0;
                (*out_result).hops = 0;
            }
            NERVUSDB_OK
        }
    }
}

/// Free a path result
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_free_path_result(result: *mut nervusdb_path_result) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = &*result;
        if !r.path.is_null() && r.path_len > 0 {
            let _ = Vec::from_raw_parts(r.path, r.path_len, r.path_len);
        }
    }
}

/// PageRank result entry
#[repr(C)]
pub struct nervusdb_pagerank_entry {
    pub node_id: u64,
    pub score: f64,
}

/// PageRank result
#[repr(C)]
pub struct nervusdb_pagerank_result {
    pub entries: *mut nervusdb_pagerank_entry,
    pub entries_len: usize,
    pub iterations: usize,
    pub converged: bool,
}

/// Compute PageRank for the graph
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_pagerank(
    db: *mut nervusdb_db,
    predicate_id: u64,
    has_predicate: bool,
    damping: f64,
    max_iterations: usize,
    tolerance: f64,
    out_result: *mut nervusdb_pagerank_result,
    out_error: *mut *mut nervusdb_error,
) -> nervusdb_status {
    clear_error(out_error);

    if out_result.is_null() {
        set_error(
            out_error,
            NERVUSDB_ERR_INVALID_ARGUMENT,
            "out_result is null",
        );
        return NERVUSDB_ERR_INVALID_ARGUMENT;
    }

    let db = match db_from_ptr(db, out_error) {
        Ok(db) => db,
        Err(code) => return code,
    };

    let pred = if has_predicate {
        Some(predicate_id)
    } else {
        None
    };

    let options = PageRankOptions {
        damping,
        max_iterations,
        tolerance,
    };

    let result = pagerank(db.get_store(), pred, options);

    // Convert HashMap to array
    let entries: Vec<nervusdb_pagerank_entry> = result
        .scores
        .into_iter()
        .map(|(node_id, score)| nervusdb_pagerank_entry { node_id, score })
        .collect();

    let mut entries_box = entries.into_boxed_slice();
    let entries_ptr = entries_box.as_mut_ptr();
    let entries_len = entries_box.len();
    std::mem::forget(entries_box);

    unsafe {
        (*out_result).entries = entries_ptr;
        (*out_result).entries_len = entries_len;
        (*out_result).iterations = result.iterations;
        (*out_result).converged = result.converged;
    }

    NERVUSDB_OK
}

/// Free a PageRank result
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nervusdb_free_pagerank_result(result: *mut nervusdb_pagerank_result) {
    if result.is_null() {
        return;
    }
    unsafe {
        let r = &*result;
        if !r.entries.is_null() && r.entries_len > 0 {
            let _ = Vec::from_raw_parts(r.entries, r.entries_len, r.entries_len);
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
            let status = nervusdb_open(path_c.as_ptr(), &mut db_ptr, ptr::null_mut());
            assert_eq!(status, NERVUSDB_OK);
            assert!(!db_ptr.is_null());

            let mut id = 0u64;
            let name = CString::new("Alice").unwrap();
            let status = nervusdb_intern(db_ptr, name.as_ptr(), &mut id, ptr::null_mut());
            assert_eq!(status, NERVUSDB_OK);
            assert!(id > 0);

            let status = nervusdb_add_triple(db_ptr, id, id, id, ptr::null_mut());
            assert_eq!(status, NERVUSDB_OK);

            let query_status = nervusdb_query_triples(
                db_ptr,
                ptr::null(),
                Some(collect),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            assert_eq!(query_status, NERVUSDB_OK);
            assert!(CALLBACK_COUNT.load(Ordering::SeqCst) >= 1);

            nervusdb_close(db_ptr);
        }
    }
}

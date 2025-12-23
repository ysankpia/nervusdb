//! Benchmark Cypher C API: JSON (exec_cypher) vs stmt (prepare/step/column).
//!
//! Run with:
//!   cargo run --example bench_cypher_ffi -p nervusdb-core --release

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::time::Instant;

use nervusdb_core::{Database, Options, Triple};
use tempfile::tempdir;

#[cfg(not(target_arch = "wasm32"))]
use nervusdb_core::ffi::{
    NERVUSDB_DONE, NERVUSDB_OK, NERVUSDB_ROW, nervusdb_close, nervusdb_column_node_id,
    nervusdb_column_relationship, nervusdb_db, nervusdb_error, nervusdb_exec_cypher,
    nervusdb_finalize, nervusdb_free_error, nervusdb_free_string, nervusdb_open,
    nervusdb_prepare_v2, nervusdb_step, nervusdb_stmt,
};

const PERSON_NODE_COUNT: usize = 100;
const EDGES_PER_PERSON: usize = 500;
const EDGE_COUNT: usize = PERSON_NODE_COUNT * EDGES_PER_PERSON;

#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("bench_cypher_ffi is not supported on wasm32");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!("==============================================");
    println!("  Cypher C API Benchmark (JSON vs stmt)");
    println!(
        "  Data: {} edges ({} Person nodes x {} edges), Query: MATCH (a:Person)-[r:KNOWS]->(b)",
        EDGE_COUNT, PERSON_NODE_COUNT, EDGES_PER_PERSON
    );
    println!("==============================================");

    let tmp = tempdir().unwrap();
    let db_base = tmp.path().join("bench");

    populate(&db_base);

    unsafe {
        let mut db_ptr: *mut nervusdb_db = ptr::null_mut();
        let mut err_ptr: *mut nervusdb_error = ptr::null_mut();
        let path_c = CString::new(db_base.to_string_lossy().as_bytes()).unwrap();

        let status = nervusdb_open(path_c.as_ptr(), &mut db_ptr, &mut err_ptr);
        if status != NERVUSDB_OK {
            panic!("nervusdb_open failed: {}", take_error(err_ptr));
        }

        let query = CString::new("MATCH (a:Person)-[r:KNOWS]->(b) RETURN a, r, b").unwrap();

        // JSON (exec_cypher)
        let mut json_ptr: *mut c_char = ptr::null_mut();
        let start = Instant::now();
        let status = nervusdb_exec_cypher(
            db_ptr,
            query.as_ptr(),
            ptr::null(),
            &mut json_ptr,
            &mut err_ptr,
        );
        let json_duration = start.elapsed();
        if status != NERVUSDB_OK {
            panic!("nervusdb_exec_cypher failed: {}", take_error(err_ptr));
        }
        let json_len = if json_ptr.is_null() {
            0
        } else {
            CStr::from_ptr(json_ptr).to_bytes().len()
        };
        nervusdb_free_string(json_ptr);

        // stmt (prepare/step/column)
        let mut stmt_ptr: *mut nervusdb_stmt = ptr::null_mut();
        let start = Instant::now();
        let status = nervusdb_prepare_v2(
            db_ptr,
            query.as_ptr(),
            ptr::null(),
            &mut stmt_ptr,
            &mut err_ptr,
        );
        let prepare_duration = start.elapsed();
        if status != NERVUSDB_OK {
            panic!("nervusdb_prepare_v2 failed: {}", take_error(err_ptr));
        }

        let iter_start = Instant::now();
        let mut rows = 0usize;
        let mut checksum: u64 = 0;
        loop {
            let rc = nervusdb_step(stmt_ptr, &mut err_ptr);
            if rc == NERVUSDB_ROW {
                rows += 1;
                let a = nervusdb_column_node_id(stmt_ptr, 0);
                let r = nervusdb_column_relationship(stmt_ptr, 1);
                let b = nervusdb_column_node_id(stmt_ptr, 2);
                checksum ^= a ^ b ^ r.subject_id ^ r.predicate_id ^ r.object_id;
                continue;
            }
            if rc == NERVUSDB_DONE {
                break;
            }
            panic!("nervusdb_step failed: {}", take_error(err_ptr));
        }
        let iter_duration = iter_start.elapsed();
        nervusdb_finalize(stmt_ptr);

        let total_duration = prepare_duration + iter_duration;
        println!(
            "[stmt] prepare {:.2?}, iter {:.2?}, total {:.2?} ({:.0} rows/sec, checksum {})",
            prepare_duration,
            iter_duration,
            total_duration,
            rows as f64 / total_duration.as_secs_f64(),
            checksum
        );

        println!(
            "[exec_cypher] {:.2?} ({:.0} rows/sec, {} bytes JSON, {:.1} bytes/row)",
            json_duration,
            rows as f64 / json_duration.as_secs_f64(),
            json_len,
            json_len as f64 / rows.max(1) as f64
        );

        nervusdb_close(db_ptr);
    }
}

fn populate(db_base: &std::path::Path) {
    let mut db = Database::open(Options::new(db_base)).unwrap();
    let type_id = db.intern("type").unwrap();
    let person_id = db.intern("Person").unwrap();
    let knows_id = db.intern("KNOWS").unwrap();

    let person_nodes: Vec<u64> = (1..=PERSON_NODE_COUNT as u64).collect();
    let mut triples = Vec::with_capacity(PERSON_NODE_COUNT + EDGE_COUNT);

    // Label the person nodes: (n, type, Person)
    for &node in &person_nodes {
        triples.push(Triple::new(node, type_id, person_id));
    }

    // Add edges from Person nodes: (n, KNOWS, o)
    let mut next_object_id = PERSON_NODE_COUNT as u64 + 1;
    for &node in &person_nodes {
        for _ in 0..EDGES_PER_PERSON {
            triples.push(Triple::new(node, knows_id, next_object_id));
            next_object_id += 1;
        }
    }

    db.batch_insert(&triples).unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn take_error(err_ptr: *mut nervusdb_error) -> String {
    if err_ptr.is_null() {
        return "unknown error (err_ptr is null)".to_string();
    }
    let (code, message) = unsafe {
        let code = (*err_ptr).code;
        let message = if (*err_ptr).message.is_null() {
            "<no message>".to_string()
        } else {
            CStr::from_ptr((*err_ptr).message)
                .to_string_lossy()
                .to_string()
        };
        (code, message)
    };
    unsafe {
        nervusdb_free_error(err_ptr);
    }
    format!("[code={}] {}", code, message)
}

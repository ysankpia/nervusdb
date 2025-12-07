//! Benchmark comparison: NervusDB vs SQLite vs redb
//!
//! Run with: cargo run --example bench_compare -p nervusdb-core --release

use std::time::Instant;
use tempfile::tempdir;

const N: usize = 1_000_000;
const QUERY_COUNT: usize = 10_000;

fn main() {
    println!("==============================================");
    println!("  Database Benchmark Comparison");
    println!("  Data: {} triples, Query: {} lookups", N, QUERY_COUNT);
    println!("==============================================");

    let nervusdb_results = bench_nervusdb();
    let sqlite_results = bench_sqlite();
    let redb_results = bench_redb();

    println!("\n==============================================");
    println!("  RESULTS SUMMARY");
    println!("==============================================");
    println!(
        "{:<15} {:>15} {:>15} {:>15}",
        "Database", "Insert/sec", "S?? Query/sec", "??O Query/sec"
    );
    println!("{:-<60}", "");
    println!(
        "{:<15} {:>15.0} {:>15.0} {:>15.0}",
        "NervusDB", nervusdb_results.0, nervusdb_results.1, nervusdb_results.2
    );
    println!(
        "{:<15} {:>15.0} {:>15.0} {:>15.0}",
        "SQLite", sqlite_results.0, sqlite_results.1, sqlite_results.2
    );
    println!(
        "{:<15} {:>15.0} {:>15.0} {:>15.0}",
        "redb (raw)", redb_results.0, redb_results.1, redb_results.2
    );
    println!("{:-<60}", "");
}

fn bench_nervusdb() -> (f64, f64, f64) {
    use nervusdb_core::{Database, Options, QueryCriteria, Triple};

    println!("\n[NervusDB] Starting benchmark...");
    let tmp = tempdir().unwrap();
    let mut db = Database::open(Options::new(tmp.path())).unwrap();

    // Pre-generate strings
    let subjects: Vec<String> = (0..N).map(|i| format!("subject_{}", i)).collect();
    let objects: Vec<String> = (0..N).map(|i| format!("object_{}", i + 1)).collect();

    // Bulk intern strings once
    let subject_refs: Vec<&str> = subjects.iter().map(|s| s.as_str()).collect();
    let object_refs: Vec<&str> = objects.iter().map(|s| s.as_str()).collect();
    let predicate_id = db.intern("knows").unwrap();
    let subject_ids = db.bulk_intern(&subject_refs).unwrap();
    let object_ids = db.bulk_intern(&object_refs).unwrap();

    // Insert using ID-based batch_insert
    let start = Instant::now();
    let triples_vec: Vec<_> = subject_ids
        .iter()
        .zip(object_ids.iter())
        .map(|(s, o)| Triple::new(*s, predicate_id, *o))
        .collect();
    let _inserted = db.batch_insert(&triples_vec).unwrap();
    let insert_duration = start.elapsed();
    let insert_rate = N as f64 / insert_duration.as_secs_f64();
    println!(
        "[NervusDB] Insert: {:.2?} ({:.0}/sec)",
        insert_duration, insert_rate
    );

    // S?? Query (using cached IDs)
    let step = (N / QUERY_COUNT).max(1);
    let start = Instant::now();
    for i in (0..N).step_by(step).take(QUERY_COUNT) {
        let criteria = QueryCriteria {
            subject_id: Some(subject_ids[i]),
            predicate_id: None,
            object_id: None,
        };
        let _results: Vec<_> = db.query(criteria).collect();
    }
    let query_s_duration = start.elapsed();
    let query_s_rate = QUERY_COUNT as f64 / query_s_duration.as_secs_f64();
    println!(
        "[NervusDB] S?? Query: {:.2?} ({:.0}/sec)",
        query_s_duration, query_s_rate
    );

    // ??O Query (using cached IDs)
    let start = Instant::now();
    for i in (0..N).step_by(step).take(QUERY_COUNT) {
        let criteria = QueryCriteria {
            subject_id: None,
            predicate_id: None,
            object_id: Some(object_ids[i]),
        };
        let _results: Vec<_> = db.query(criteria).collect();
    }
    let query_o_duration = start.elapsed();
    let query_o_rate = QUERY_COUNT as f64 / query_o_duration.as_secs_f64();
    println!(
        "[NervusDB] ??O Query: {:.2?} ({:.0}/sec)",
        query_o_duration, query_o_rate
    );

    (insert_rate, query_s_rate, query_o_rate)
}

fn bench_sqlite() -> (f64, f64, f64) {
    use rusqlite::Connection;

    println!("\n[SQLite] Starting benchmark...");
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("bench.db");
    let conn = Connection::open(&db_path).unwrap();

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         CREATE TABLE triples (
             id INTEGER PRIMARY KEY,
             subject TEXT NOT NULL,
             predicate TEXT NOT NULL,
             object TEXT NOT NULL
         );
         CREATE INDEX idx_subject ON triples(subject);
         CREATE INDEX idx_object ON triples(object);",
    )
    .unwrap();

    // Pre-generate strings
    let subjects: Vec<String> = (0..N).map(|i| format!("subject_{}", i)).collect();
    let objects: Vec<String> = (0..N).map(|i| format!("object_{}", i + 1)).collect();

    // Insert
    let start = Instant::now();
    conn.execute("BEGIN TRANSACTION", []).unwrap();
    {
        let mut stmt = conn
            .prepare("INSERT INTO triples (subject, predicate, object) VALUES (?1, ?2, ?3)")
            .unwrap();
        for i in 0..N {
            stmt.execute([&subjects[i], "knows", &objects[i]]).unwrap();
        }
    }
    conn.execute("COMMIT", []).unwrap();
    let insert_duration = start.elapsed();
    let insert_rate = N as f64 / insert_duration.as_secs_f64();
    println!(
        "[SQLite] Insert: {:.2?} ({:.0}/sec)",
        insert_duration, insert_rate
    );

    // S?? Query
    let step = (N / QUERY_COUNT).max(1);
    let start = Instant::now();
    {
        let mut stmt = conn
            .prepare("SELECT subject, predicate, object FROM triples WHERE subject = ?1")
            .unwrap();
        for i in (0..N).step_by(step).take(QUERY_COUNT) {
            let _results: Vec<(String, String, String)> = stmt
                .query_map([&subjects[i]], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
        }
    }
    let query_s_duration = start.elapsed();
    let query_s_rate = QUERY_COUNT as f64 / query_s_duration.as_secs_f64();
    println!(
        "[SQLite] S?? Query: {:.2?} ({:.0}/sec)",
        query_s_duration, query_s_rate
    );

    // ??O Query
    let start = Instant::now();
    {
        let mut stmt = conn
            .prepare("SELECT subject, predicate, object FROM triples WHERE object = ?1")
            .unwrap();
        for i in (0..N).step_by(step).take(QUERY_COUNT) {
            let _results: Vec<(String, String, String)> = stmt
                .query_map([&objects[i]], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
        }
    }
    let query_o_duration = start.elapsed();
    let query_o_rate = QUERY_COUNT as f64 / query_o_duration.as_secs_f64();
    println!(
        "[SQLite] ??O Query: {:.2?} ({:.0}/sec)",
        query_o_duration, query_o_rate
    );

    (insert_rate, query_s_rate, query_o_rate)
}

fn bench_redb() -> (f64, f64, f64) {
    use redb::{Database, MultimapTableDefinition, ReadableDatabase, TableDefinition};

    println!("\n[redb] Starting benchmark...");
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("bench.redb");
    let db = Database::create(&db_path).unwrap();

    const SPO_TABLE: TableDefinition<&str, ()> = TableDefinition::new("spo");
    const SUBJECT_IDX: MultimapTableDefinition<&str, &str> =
        MultimapTableDefinition::new("subject_idx");
    const OBJECT_IDX: MultimapTableDefinition<&str, &str> =
        MultimapTableDefinition::new("object_idx");

    // Pre-generate strings
    let subjects: Vec<String> = (0..N).map(|i| format!("subject_{}", i)).collect();
    let objects: Vec<String> = (0..N).map(|i| format!("object_{}", i + 1)).collect();
    let predicate = "knows";

    // Insert - open tables once, reuse handles
    let start = Instant::now();
    let write_txn = db.begin_write().unwrap();
    {
        let mut spo_table = write_txn.open_table(SPO_TABLE).unwrap();
        let mut subject_idx = write_txn.open_multimap_table(SUBJECT_IDX).unwrap();
        let mut object_idx = write_txn.open_multimap_table(OBJECT_IDX).unwrap();

        for i in 0..N {
            let spo_key = format!("{}:{}:{}", &subjects[i], predicate, &objects[i]);
            let po_value = format!("{}:{}", predicate, &objects[i]);
            let sp_value = format!("{}:{}", &subjects[i], predicate);

            spo_table.insert(spo_key.as_str(), ()).unwrap();
            subject_idx
                .insert(subjects[i].as_str(), po_value.as_str())
                .unwrap();
            object_idx
                .insert(objects[i].as_str(), sp_value.as_str())
                .unwrap();
        }
    }
    write_txn.commit().unwrap();
    let insert_duration = start.elapsed();
    let insert_rate = N as f64 / insert_duration.as_secs_f64();
    println!(
        "[redb] Insert: {:.2?} ({:.0}/sec)",
        insert_duration, insert_rate
    );

    // S?? Query
    let step = (N / QUERY_COUNT).max(1);
    let start = Instant::now();
    {
        let read_txn = db.begin_read().unwrap();
        let subject_idx = read_txn.open_multimap_table(SUBJECT_IDX).unwrap();

        for i in (0..N).step_by(step).take(QUERY_COUNT) {
            let _results: Vec<_> = subject_idx
                .get(subjects[i].as_str())
                .unwrap()
                .map(|r| r.unwrap().value().to_string())
                .collect();
        }
    }
    let query_s_duration = start.elapsed();
    let query_s_rate = QUERY_COUNT as f64 / query_s_duration.as_secs_f64();
    println!(
        "[redb] S?? Query: {:.2?} ({:.0}/sec)",
        query_s_duration, query_s_rate
    );

    // ??O Query
    let start = Instant::now();
    {
        let read_txn = db.begin_read().unwrap();
        let object_idx = read_txn.open_multimap_table(OBJECT_IDX).unwrap();

        for i in (0..N).step_by(step).take(QUERY_COUNT) {
            let _results: Vec<_> = object_idx
                .get(objects[i].as_str())
                .unwrap()
                .map(|r| r.unwrap().value().to_string())
                .collect();
        }
    }
    let query_o_duration = start.elapsed();
    let query_o_rate = QUERY_COUNT as f64 / query_o_duration.as_secs_f64();
    println!(
        "[redb] ??O Query: {:.2?} ({:.0}/sec)",
        query_o_duration, query_o_rate
    );

    (insert_rate, query_s_rate, query_o_rate)
}

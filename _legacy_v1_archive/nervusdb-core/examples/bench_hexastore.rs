use nervusdb_core::{Database, Fact, Options, QueryCriteria};
use std::time::Instant;
use tempfile::tempdir;

fn main() {
    let tmp = tempdir().unwrap();
    let mut db = Database::open(Options::new(tmp.path())).unwrap();

    let n = 100_000;
    println!("Generating and inserting {} triples...", n);

    // Use transaction for batch insert (10-100x faster than individual inserts)
    let start = Instant::now();
    db.begin_transaction().unwrap();
    for i in 0..n {
        let s = format!("subject_{}", i);
        let p = "knows";
        let o = format!("object_{}", i + 1);
        db.add_fact(Fact::new(&s, p, &o)).unwrap();
    }
    db.commit_transaction().unwrap();
    let duration = start.elapsed();
    println!("Insert time: {:.2?}", duration);
    println!(
        "Insert rate: {:.0} triples/sec",
        n as f64 / duration.as_secs_f64()
    );

    // Test S?? Query (using SPO index)
    println!("\nBenchmarking S?? queries (10,000 random lookups)...");
    let start = Instant::now();
    for i in (0..n).step_by(10) {
        let s_id = db.resolve_id(&format!("subject_{}", i)).unwrap();
        let criteria = QueryCriteria {
            subject_id: s_id,
            predicate_id: None,
            object_id: None,
        };
        let results: Vec<_> = db.query(criteria).collect();
        assert_eq!(results.len(), 1);
    }
    let duration = start.elapsed();
    println!("S?? Query time: {:.2?}", duration);
    println!(
        "S?? Query rate: {:.0} queries/sec",
        10_000.0 / duration.as_secs_f64()
    );

    // Test ??O Query (using OSP/OPS index)
    // This would be O(N) per query in the old implementation, making this loop O(N*M) -> super slow
    println!("\nBenchmarking ??O queries (10,000 random lookups)...");
    let start = Instant::now();
    for i in (0..n).step_by(10) {
        let o_id = db.resolve_id(&format!("object_{}", i + 1)).unwrap();
        let criteria = QueryCriteria {
            subject_id: None,
            predicate_id: None,
            object_id: o_id,
        };
        let results: Vec<_> = db.query(criteria).collect();
        assert_eq!(results.len(), 1);
    }
    let duration = start.elapsed();
    println!("??O Query time: {:.2?}", duration);
    println!(
        "??O Query rate: {:.0} queries/sec",
        10_000.0 / duration.as_secs_f64()
    );
}

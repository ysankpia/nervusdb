use nervusdb_core::{EpisodeInput, TemporalStore};
use redb::Database;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;

fn main() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.redb");
    let redb = Arc::new(Database::create(&path).unwrap());
    let mut store = TemporalStore::open(redb).unwrap();

    let start = Instant::now();
    let count = 10_000;

    for i in 0..count {
        store
            .add_episode(EpisodeInput {
                source_type: "benchmark".into(),
                payload: Value::String(format!("payload {}", i).into()),
                occurred_at: "2025-01-01T00:00:00Z".into(),
                trace_hash: None,
            })
            .unwrap();

        if i % 1000 == 0 {
            print!(".");
            use std::io::Write;
            std::io::stdout().flush().unwrap();
        }
    }
    println!();

    let duration = start.elapsed();
    println!(
        "Inserted {} episodes in {:?}. Rate: {:.2} eps",
        count,
        duration,
        count as f64 / duration.as_secs_f64()
    );

    let metadata = std::fs::metadata(&path).unwrap();
    println!("Store file size: {} bytes", metadata.len());
}

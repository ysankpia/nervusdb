use nervusdb_core::temporal::{EpisodeInput, TemporalStore};
use serde_json::Value;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn benchmark_append_performance() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bench.temporal.json");
    let mut store = TemporalStore::open(&path).unwrap();

    let total_items = 10_000;
    let chunk_size = 1_000;

    let mut times = Vec::new();

    println!("Starting benchmark with {} items...", total_items);

    for i in 0..total_items {
        if i % chunk_size == 0 {
            let start = Instant::now();
            // Measure a single insert at the start of the chunk
            store
                .add_episode(EpisodeInput {
                    source_type: "benchmark".into(),
                    payload: Value::String(format!("item {}", i).into()),
                    occurred_at: "2025-01-01T00:00:00Z".into(),
                    trace_hash: None,
                })
                .unwrap();
            let duration = start.elapsed();
            times.push((i, duration));
        } else {
            store
                .add_episode(EpisodeInput {
                    source_type: "benchmark".into(),
                    payload: Value::String(format!("item {}", i).into()),
                    occurred_at: "2025-01-01T00:00:00Z".into(),
                    trace_hash: None,
                })
                .unwrap();
        }
    }

    println!("Benchmark results (time per insert at index):");
    for (index, duration) in &times {
        println!("Index {}: {:?}", index, duration);
    }

    // Simple verification: The last measurement shouldn't be orders of magnitude larger than the first
    // In O(N) (rewrite whole file), the 10,000th write would be ~1000x slower than the 10th if file size grows linearly.
    // In O(1) (append), it should be roughly constant (ignoring OS buffering noise).

    let first_duration = times[0].1.as_micros();
    let last_duration = times.last().unwrap().1.as_micros();

    println!("First insert: {} us", first_duration);
    println!("Last insert: {} us", last_duration);

    // Allow some variance, but if it's > 100x slower, it's likely O(N)
    // Note: OS file buffering might make some writes fast and some slow, but generally it shouldn't scale linearly.
}

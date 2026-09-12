//! Focused investigation: why is a LARGER buffer pool SLOWER for edge writes?
//!
//! Runs the identical workload at several pool sizes, in a fixed order, and
//! reports per-pool spill/evict/miss so the cause can be attributed rather than
//! guessed.

use nervusdb::NervusDb;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nodes: u64 = std::env::var("GL_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);
    let edges: u64 = std::env::var("GL_EDGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(800_000);

    println!("nodes={} edges={}", nodes, edges);
    println!(
        "{:>8}  {:>12}  {:>10}  {:>10}  {:>10}  {:>10}",
        "frames", "ops/s", "miss/e", "spill/e", "evict/e", "ckpt ms"
    );

    for &frames in &[256usize, 1024, 4096, 16384] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("probe.db");
        let db = NervusDb::open_with_pool_size(&path, frames)?;

        db.with_transaction(|tx| {
            for i in 1..=nodes {
                let mut m = HashMap::new();
                m.insert("idx".to_string(), nervusdb::Value::from(i as i64));
                tx.add_node(HashSet::from(["N".to_string()]), m)?;
            }
            Ok(())
        })?;

        let before = db.buffer_stats();
        // GL_CHUNKS splits the edge workload into N transactions. A single huge
        // transaction materialises the whole batch plan in memory; chunking
        // isolates whether the pool-size inversion is caused by that footprint
        // or by the pool itself.
        let chunks: u64 = std::env::var("GL_CHUNKS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let per = edges.div_ceil(chunks);

        let t = Instant::now();
        let mut base = 0u64;
        while base < edges {
            let n = per.min(edges - base);
            db.with_transaction(|tx| {
                for i in 0..n {
                    let g = base + i;
                    let src = (g % nodes) + 1;
                    let dst = ((g * 7919 + 13) % nodes) + 1;
                    if src != dst {
                        tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
                    }
                }
                Ok(())
            })?;
            base += n;
        }
        let el = t.elapsed();
        let after = db.buffer_stats();

        let ck = Instant::now();
        db.checkpoint()?;
        let ck_ms = ck.elapsed().as_secs_f64() * 1000.0;

        println!(
            "{:>8}  {:>12.0}  {:>10.3}  {:>10.3}  {:>10.3}  {:>10.1}",
            frames,
            edges as f64 / el.as_secs_f64(),
            (after.cache_misses - before.cache_misses) as f64 / edges as f64,
            (after.spill_count - before.spill_count) as f64 / edges as f64,
            (after.evictions - before.evictions) as f64 / edges as f64,
            ck_ms
        );
    }

    Ok(())
}

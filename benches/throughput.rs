//! Reproducible throughput benchmark.
//!
//! Run with:
//! ```bash
//! cargo bench --bench throughput                # all scenarios
//! GL_SCALE=small cargo bench --bench throughput # quick smoke run
//! GL_MAX_ACTIONS=4000000 cargo bench --bench throughput # keep the engine's cap
//! ```
//!
//! Scenarios are read from `GL_SCALE` so the same code can be used for a fast
//! sanity check and for the full documented run. Every scenario reports its own
//! configuration alongside the result, so numbers can never be quoted without
//! their measurement conditions.
//!
//! The documented scenarios queue a whole batch in **one** transaction, which is the
//! measurement condition the published figures were taken under. The 10M-node scenario
//! therefore exceeds `DEFAULT_MAX_TRANSACTION_ACTIONS` and the engine rejects it, by
//! design. The benchmark lifts the cap for itself (see `GL_MAX_ACTIONS`) rather than
//! chunking, because chunking would measure a different workload than the figure claims
//! to.

use nervusdb::{NervusDb, NervusDbOptions, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Measurement configuration for one scenario.
struct Config {
    label: &'static str,
    nodes: u64,
    edges: u64,
    pool_frames: usize,
    in_memory: bool,
    /// When false the database file is kept, so the caller can inspect on-disk size.
    cleanup: bool,
}

fn make_props(i: u64) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    // GL_NO_PROPS=1 measures the ceiling with no property payload at all, which
    // separates record-writing cost from slotted-property-page cost.
    if std::env::var("GL_NO_PROPS").is_ok() {
        return m;
    }
    m.insert("idx".to_string(), Value::from(i as i64));
    m.insert("name".to_string(), Value::from(format!("entity-{:08}", i)));
    m
}

/// Run one node/edge write scenario and print a result block.
fn run(cfg: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path: PathBuf = if cfg.in_memory {
        PathBuf::from(":memory:")
    } else {
        dir.path().join("bench.db")
    };

    // The documented scenarios are **single-transaction** batched writes — that is
    // the measurement condition the figures were taken under, not an accident. The
    // 10M-node scenario therefore queues 10M actions and exceeds
    // `DEFAULT_MAX_TRANSACTION_ACTIONS` (4M), which the engine rejects on purpose.
    //
    // Chunking here would be the wrong fix: it would silently change what is being
    // measured, and the published figures would then describe a different workload
    // than the one they were taken from. So the cap is lifted deliberately, and only
    // here. `GL_MAX_ACTIONS` lets a constrained machine set a real bound and see the
    // rejection instead.
    let options = NervusDbOptions {
        buffer_pool_frames: cfg.pool_frames,
        max_transaction_actions: match std::env::var("GL_MAX_ACTIONS") {
            Ok(v) => v.parse().unwrap_or(0),
            Err(_) => 0,
        },
        ..NervusDbOptions::default()
    };
    let db = NervusDb::open_with_options(&path, options)?;

    // ---------- nodes ----------
    let node_start = Instant::now();
    db.with_transaction(|tx| {
        for i in 1..=cfg.nodes {
            tx.add_node(HashSet::from(["Entity".to_string()]), make_props(i))?;
        }
        Ok(())
    })?;
    let node_elapsed = node_start.elapsed();
    let node_rate = cfg.nodes as f64 / node_elapsed.as_secs_f64();

    // ---------- edges (only when requested) ----------
    let (edge_rate, edge_elapsed, edge_burst_rate) = if cfg.edges > 0 {
        let edge_start = Instant::now();
        db.with_transaction(|tx| {
            for i in 0..cfg.edges {
                let src = (i % cfg.nodes) + 1;
                let dst = ((i * 7_919 + 13) % cfg.nodes) + 1;
                if src != dst {
                    tx.add_edge(src, dst, "REL", HashMap::new(), 1.0)?;
                }
            }
            Ok(())
        })?;
        let el = edge_start.elapsed();
        // A 50k random-edge burst measured on its own, matching the documented
        // "spike" figure, so the two are directly comparable.
        let burst = if cfg.edges >= 50_000 {
            let burst_start = Instant::now();
            db.with_transaction(|tx| {
                for i in 0..50_000u64 {
                    let src = ((i * 31) % cfg.nodes) + 1;
                    let dst = ((i * 999_983 + 7) % cfg.nodes) + 1;
                    if src != dst {
                        tx.add_edge(src, dst, "BURST", HashMap::new(), 1.0)?;
                    }
                }
                Ok(())
            })?;
            let b_el = burst_start.elapsed();
            Some((
                b_el,
                50_000f64 / b_el.as_secs_f64(),
                b_el.as_secs_f64() * 1000.0,
            ))
        } else {
            None
        };
        (
            cfg.edges as f64 / el.as_secs_f64(),
            Some(el),
            burst.map(|(d, r, ms)| (r, d, ms)),
        )
    } else {
        (0.0, None, None)
    };

    // ---------- checkpoint (measures the flush path too) ----------
    let ckpt_start = Instant::now();
    db.checkpoint()?;
    let ckpt_elapsed = ckpt_start.elapsed();

    let file_size = if cfg.in_memory {
        0
    } else {
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    };
    let stats = db.buffer_stats();

    println!("\n=== {} ===", cfg.label);
    println!(
        "config      : {} nodes, {} edges, pool {} frames ({} MB), {}",
        cfg.nodes,
        cfg.edges,
        cfg.pool_frames,
        cfg.pool_frames * 4 / 1024,
        if cfg.in_memory {
            ":memory:"
        } else {
            "file-backed"
        }
    );
    println!(
        "nodes       : {:>12.0} ops/s   ({:.2?} for {} nodes)",
        node_rate, node_elapsed, cfg.nodes
    );
    if let Some(el) = edge_elapsed {
        println!(
            "edges       : {:>12.0} ops/s   ({:.2?} for {} edges)",
            edge_rate, el, cfg.edges
        );
    }
    if let Some((rate, dur, ms)) = edge_burst_rate {
        println!(
            "50k burst   : {:>12.0} ops/s   ({:.2?} = {:.1} ms for 50,000 edges)",
            rate, dur, ms
        );
    }
    println!("checkpoint  : {:.2?}", ckpt_elapsed);
    if !cfg.in_memory {
        println!(
            "file size   : {:.2} MB ({:.1} bytes/entity)",
            file_size as f64 / 1_048_576.0,
            file_size as f64 / (cfg.nodes + cfg.edges) as f64
        );
    }
    println!(
        "buffer      : hit {:.1}%, spill {}, evict {}",
        stats.hit_rate_percentage, stats.spill_count, stats.evictions
    );

    if cfg.cleanup {
        drop(db);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scale = std::env::var("GL_SCALE").unwrap_or_else(|_| "full".to_string());
    let quick = scale == "small";

    println!("NervusDb throughput benchmark (scale = {})", scale);
    println!("host: {}", std::env::consts::OS);
    if quick {
        println!("NOTE: smoke scale, not the documented figures.");
    }

    // 1. 10M nodes, file-backed, 64MB pool
    run(&Config {
        label: "10M nodes, file-backed, 64MB pool",
        nodes: if quick { 100_000 } else { 10_000_000 },
        edges: 0,
        pool_frames: 16_384,
        in_memory: false,
        cleanup: true,
    })?;

    // 2. pure in-memory node write
    run(&Config {
        label: "1M nodes, :memory:",
        nodes: if quick { 100_000 } else { 1_000_000 },
        edges: 0,
        pool_frames: 16_384,
        in_memory: true,
        cleanup: true,
    })?;

    // 3+4. 1M nodes + 4M discrete edges, then a 50k burst
    run(&Config {
        label: "1M nodes + 4M discrete edges, file-backed, 64MB pool",
        nodes: if quick { 20_000 } else { 1_000_000 },
        edges: if quick { 80_000 } else { 4_000_000 },
        pool_frames: 16_384,
        in_memory: false,
        cleanup: true,
    })?;

    // 5. the same workload under the 1MB floor, to show the constrained case
    run(&Config {
        label: "1M nodes + 4M discrete edges, file-backed, 1MB pool",
        nodes: if quick { 20_000 } else { 1_000_000 },
        edges: if quick { 80_000 } else { 4_000_000 },
        pool_frames: 256,
        in_memory: false,
        cleanup: true,
    })?;

    let _ = Duration::from_secs(0);
    Ok(())
}

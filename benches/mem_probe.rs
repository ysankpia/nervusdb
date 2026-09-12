//! Measures the peak memory footprint of a single large batch weave, to
//! attribute the "larger pool is slower" inversion seen in the full-scale run.
//!
//! Prints RSS before/after one 4M-edge transaction so the planner's footprint is
//! visible independently of the buffer pool.

use nervusdb::NervusDb;
use std::collections::{HashMap, HashSet};

/// Resident set size in KB, read from the OS (macOS `ps`, Linux `/proc`).
fn rss_kb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(pages) = s.split_whitespace().nth(1) {
                if let Ok(p) = pages.parse::<u64>() {
                    return p * 4; // 4KB pages
                }
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS: ask ps for our own RSS
        let pid = std::process::id();
        if let Ok(out) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
        {
            if let Ok(s) = String::from_utf8(out.stdout) {
                return s.trim().parse().unwrap_or(0);
            }
        }
        0
    }
}

fn mb(kb: u64) -> f64 {
    kb as f64 / 1024.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nodes: u64 = std::env::var("GL_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000);
    let edges: u64 = std::env::var("GL_EDGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4_000_000);
    let frames: usize = std::env::var("GL_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16_384);

    println!("nodes={} edges={} frames={}", nodes, edges, frames);

    let dir = tempfile::tempdir()?;
    let db = NervusDb::open_with_pool_size(dir.path().join("mem.db"), frames)?;

    println!("RSS at start          : {:>8.1} MB", mb(rss_kb()));

    db.with_transaction(|tx| {
        for i in 1..=nodes {
            let mut m = HashMap::new();
            m.insert("idx".to_string(), nervusdb::Value::from(i as i64));
            tx.add_node(HashSet::from(["N".to_string()]), m)?;
        }
        Ok(())
    })?;
    println!("RSS after {} nodes : {:>8.1} MB", nodes, mb(rss_kb()));

    // Build the whole edge plan in one transaction, as the full-scale bench does.
    let mut tx = db.begin_transaction()?;
    for i in 0..edges {
        let src = (i % nodes) + 1;
        let dst = ((i * 7919 + 13) % nodes) + 1;
        if src != dst {
            tx.add_edge(src, dst, "R", HashMap::new(), 1.0)?;
        }
    }
    println!(
        "RSS after queueing {} edges (BEFORE commit): {:>8.1} MB",
        edges,
        mb(rss_kb())
    );

    let peak_before = rss_kb();
    tx.commit()?;
    let after = rss_kb();
    println!("RSS after commit      : {:>8.1} MB", mb(after));
    println!(
        "commit-phase peak delta: {:>8.1} MB  <-- planner structures allocated here",
        mb(after.saturating_sub(peak_before))
    );

    Ok(())
}

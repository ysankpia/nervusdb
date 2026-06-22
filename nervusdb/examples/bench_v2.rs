//! Core 0.1 benchmark driver.
//!
//! This is release evidence, not a public API example. It intentionally uses the
//! public `nervusdb::Db` facade so benchmark scripts do not depend on local
//! `publish = false` wrapper crates.

use nervusdb::{Db, GraphSnapshot, PropertyValue};
use std::time::Instant;
use tempfile::tempdir;

#[derive(Debug, Clone)]
struct Config {
    nodes: usize,
    degree: usize,
    iters: usize,
    write_iters: usize,
}

#[derive(Debug, Clone, Copy)]
struct NeighborBenchResult {
    edges_per_sec: f64,
    edges_total: u64,
    avg_us: f64,
    p95_us: f64,
    p99_us: f64,
}

#[derive(Debug, Clone, Copy)]
struct WriteTxnBenchResult {
    avg_us: f64,
    p95_us: f64,
    p99_us: f64,
}

#[derive(Debug, Clone, Copy)]
struct PropertyLookupBenchResult {
    rows_total: u64,
    avg_us: f64,
    p95_us: f64,
    p99_us: f64,
}

#[derive(Debug, Clone)]
struct InsertBenchResult {
    nodes: Vec<u32>,
    label: u32,
    rel: u32,
    stage_get_schema_ms: f64,
    stage_create_nodes_ms: f64,
    stage_create_edges_ms: f64,
    stage_commit_ms: f64,
}

impl InsertBenchResult {
    fn total_ms(&self) -> f64 {
        self.stage_get_schema_ms
            + self.stage_create_nodes_ms
            + self.stage_create_edges_ms
            + self.stage_commit_ms
    }
}

impl Config {
    fn from_args() -> Self {
        let mut cfg = Self {
            nodes: 50_000,
            degree: 8,
            iters: 2_000,
            write_iters: 200,
        };

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--nodes" => cfg.nodes = parse_usize(args.next()),
                "--degree" => cfg.degree = parse_usize(args.next()),
                "--iters" => cfg.iters = parse_usize(args.next()),
                "--write-iters" => cfg.write_iters = parse_usize(args.next()),
                _ => {
                    eprintln!(
                        "unknown arg: {arg}\n  supported: --nodes N --degree D --iters I --write-iters W"
                    );
                    std::process::exit(2);
                }
            }
        }

        if cfg.nodes == 0 {
            eprintln!("--nodes must be > 0");
            std::process::exit(2);
        }
        if cfg.degree == 0 {
            eprintln!("--degree must be > 0");
            std::process::exit(2);
        }
        if cfg.iters == 0 {
            eprintln!("--iters must be > 0");
            std::process::exit(2);
        }
        if cfg.write_iters == 0 {
            eprintln!("--write-iters must be > 0");
            std::process::exit(2);
        }

        cfg
    }
}

fn parse_usize(v: Option<String>) -> usize {
    v.unwrap_or_else(|| {
        eprintln!("missing value");
        std::process::exit(2);
    })
    .parse::<usize>()
    .unwrap_or_else(|_| {
        eprintln!("invalid integer");
        std::process::exit(2);
    })
}

fn percentile_us(mut samples: Vec<f64>, q: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((samples.len() - 1) as f64 * q).round() as usize;
    samples[idx]
}

fn summarize_neighbor_bench(
    edges_total: u64,
    total_secs: f64,
    latencies_us: Vec<f64>,
) -> NeighborBenchResult {
    let avg_us = if latencies_us.is_empty() {
        0.0
    } else {
        latencies_us.iter().sum::<f64>() / latencies_us.len() as f64
    };
    let p95_us = percentile_us(latencies_us.clone(), 0.95);
    let p99_us = percentile_us(latencies_us, 0.99);

    NeighborBenchResult {
        edges_per_sec: edges_total as f64 / total_secs.max(1e-9),
        edges_total,
        avg_us,
        p95_us,
        p99_us,
    }
}

fn summarize_write_txn_bench(latencies_us: Vec<f64>) -> WriteTxnBenchResult {
    let avg_us = if latencies_us.is_empty() {
        0.0
    } else {
        latencies_us.iter().sum::<f64>() / latencies_us.len() as f64
    };
    let p95_us = percentile_us(latencies_us.clone(), 0.95);
    let p99_us = percentile_us(latencies_us, 0.99);
    WriteTxnBenchResult {
        avg_us,
        p95_us,
        p99_us,
    }
}

fn summarize_property_lookup_bench(
    rows_total: u64,
    latencies_us: Vec<f64>,
) -> PropertyLookupBenchResult {
    let avg_us = if latencies_us.is_empty() {
        0.0
    } else {
        latencies_us.iter().sum::<f64>() / latencies_us.len() as f64
    };
    let p95_us = percentile_us(latencies_us.clone(), 0.95);
    let p99_us = percentile_us(latencies_us, 0.99);
    PropertyLookupBenchResult {
        rows_total,
        avg_us,
        p95_us,
        p99_us,
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn main() {
    let cfg = Config::from_args();

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("bench");
    let stage_open_start = Instant::now();
    let db = Db::open(&db_path).unwrap();
    let stage_open_ms = elapsed_ms(stage_open_start);

    let total_edges = cfg.nodes * cfg.degree;
    let insert = bench_insert(&db, cfg.clone());
    let insert_total_ms = insert.total_ms();
    let insert_edges_per_sec = total_edges as f64 / (insert_total_ms / 1_000.0).max(1e-9);

    let stage_reopen_start = Instant::now();
    drop(db);
    let db = Db::open(&db_path).unwrap();
    let snapshot = db.snapshot();
    assert_eq!(snapshot.node_count(Some(insert.label)), cfg.nodes as u64);
    assert_eq!(snapshot.edge_count(Some(insert.rel)), total_edges as u64);
    drop(snapshot);
    let stage_reopen_verify_ms = elapsed_ms(stage_reopen_start);

    let stage_neighbors_hot_start = Instant::now();
    let neighbors_hot = bench_neighbors_hot(&db, insert.nodes[0], insert.rel, cfg.iters);
    let stage_neighbors_hot_ms = elapsed_ms(stage_neighbors_hot_start);
    let stage_neighbors_cold_start = Instant::now();
    let neighbors_cold = bench_neighbors_cold(&db, &insert.nodes, insert.rel, cfg.iters);
    let stage_neighbors_cold_ms = elapsed_ms(stage_neighbors_cold_start);
    let property_lookup_iters = cfg.iters;
    let property_lookup_target = format!("node_{}", cfg.nodes - 1);
    let stage_property_lookup_scan_start = Instant::now();
    let property_lookup_scan = bench_property_lookup_scan(
        &db,
        insert.label,
        &property_lookup_target,
        property_lookup_iters,
    );
    let stage_property_lookup_scan_ms = elapsed_ms(stage_property_lookup_scan_start);
    let stage_property_lookup_index_start = Instant::now();
    let property_lookup_index = bench_property_lookup_index(
        &db,
        insert.label,
        &property_lookup_target,
        property_lookup_iters,
    );
    let stage_property_lookup_index_ms = elapsed_ms(stage_property_lookup_index_start);
    assert_eq!(
        property_lookup_scan.rows_total, property_lookup_index.rows_total,
        "scan and index lookup must return the same row count"
    );
    let property_lookup_speedup =
        stage_property_lookup_scan_ms / stage_property_lookup_index_ms.max(1e-9);
    let stage_write_txn_start = Instant::now();
    let write_txn = bench_write_txn(&db, insert.label, cfg.nodes as u64 + 1, cfg.write_iters);
    let stage_write_txn_ms = elapsed_ms(stage_write_txn_start);
    let read_query_p99_ms = neighbors_cold.p99_us / 1_000.0;
    let write_txn_p99_ms = write_txn.p99_us / 1_000.0;
    let estimated_kv_writes = (6 * cfg.nodes) + (2 * total_edges);

    println!("=== NervusDB Core 0.1 Bench ===");
    println!(
        "nodes={} degree={} edges={} iters={} write_iters={}",
        cfg.nodes, cfg.degree, total_edges, cfg.iters, cfg.write_iters
    );
    println!(
        "insert: {:.3}s ({:.0} edges/sec)",
        insert_total_ms / 1_000.0,
        insert_edges_per_sec
    );
    println!(
        "stages: open={:.2}ms schema={:.2}ms create_nodes={:.2}ms create_edges={:.2}ms commit={:.2}ms reopen_verify={:.2}ms",
        stage_open_ms,
        insert.stage_get_schema_ms,
        insert.stage_create_nodes_ms,
        insert.stage_create_edges_ms,
        insert.stage_commit_ms,
        stage_reopen_verify_ms
    );
    println!(
        "neighbors_hot: {:.0} edges/sec ({} edges, avg={:.2}us, p95={:.2}us, p99={:.2}us)",
        neighbors_hot.edges_per_sec,
        neighbors_hot.edges_total,
        neighbors_hot.avg_us,
        neighbors_hot.p95_us,
        neighbors_hot.p99_us
    );
    println!(
        "neighbors_cold: {:.0} edges/sec ({} edges, avg={:.2}us, p95={:.2}us, p99={:.2}us)",
        neighbors_cold.edges_per_sec,
        neighbors_cold.edges_total,
        neighbors_cold.avg_us,
        neighbors_cold.p95_us,
        neighbors_cold.p99_us
    );
    println!(
        "write_txn: avg={:.2}us, p95={:.2}us, p99={:.2}us ({:.4}ms)",
        write_txn.avg_us, write_txn.p95_us, write_txn.p99_us, write_txn_p99_ms
    );
    println!(
        "property_lookup: scan={:.2}ms p99={:.2}us index={:.2}ms p99={:.2}us speedup={:.2}x rows={}",
        stage_property_lookup_scan_ms,
        property_lookup_scan.p99_us,
        stage_property_lookup_index_ms,
        property_lookup_index.p99_us,
        property_lookup_speedup,
        property_lookup_index.rows_total
    );

    println!(
        "{{\"nodes\":{},\"degree\":{},\"edges\":{},\"iters\":{},\"write_iters\":{},\"stage_open_ms\":{:.3},\"stage_get_schema_ms\":{:.3},\"stage_create_nodes_ms\":{:.3},\"stage_create_edges_ms\":{:.3},\"stage_commit_ms\":{:.3},\"stage_reopen_verify_ms\":{:.3},\"stage_neighbors_hot_ms\":{:.3},\"stage_neighbors_cold_ms\":{:.3},\"stage_property_lookup_scan_ms\":{:.3},\"stage_property_lookup_index_ms\":{:.3},\"stage_write_txn_ms\":{:.3},\"insert_total_ms\":{:.3},\"insert_edges_per_sec\":{:.3},\"estimated_kv_writes\":{},\"neighbors_hot_edges_per_sec\":{:.3},\"neighbors_cold_edges_per_sec\":{:.3},\"neighbors_hot_avg_us\":{:.3},\"neighbors_hot_p95_us\":{:.3},\"neighbors_hot_p99_us\":{:.3},\"neighbors_cold_avg_us\":{:.3},\"neighbors_cold_p95_us\":{:.3},\"neighbors_cold_p99_us\":{:.3},\"property_lookup_iters\":{},\"property_lookup_rows\":{},\"property_lookup_scan_avg_us\":{:.3},\"property_lookup_scan_p95_us\":{:.3},\"property_lookup_scan_p99_us\":{:.3},\"property_lookup_index_avg_us\":{:.3},\"property_lookup_index_p95_us\":{:.3},\"property_lookup_index_p99_us\":{:.3},\"property_lookup_speedup\":{:.3},\"write_txn_avg_us\":{:.3},\"write_txn_p95_us\":{:.3},\"write_txn_p99_us\":{:.3},\"write_txn_p99_ms\":{:.6},\"read_query_p99_ms\":{:.6}}}",
        cfg.nodes,
        cfg.degree,
        total_edges,
        cfg.iters,
        cfg.write_iters,
        stage_open_ms,
        insert.stage_get_schema_ms,
        insert.stage_create_nodes_ms,
        insert.stage_create_edges_ms,
        insert.stage_commit_ms,
        stage_reopen_verify_ms,
        stage_neighbors_hot_ms,
        stage_neighbors_cold_ms,
        stage_property_lookup_scan_ms,
        stage_property_lookup_index_ms,
        stage_write_txn_ms,
        insert_total_ms,
        insert_edges_per_sec,
        estimated_kv_writes,
        neighbors_hot.edges_per_sec,
        neighbors_cold.edges_per_sec,
        neighbors_hot.avg_us,
        neighbors_hot.p95_us,
        neighbors_hot.p99_us,
        neighbors_cold.avg_us,
        neighbors_cold.p95_us,
        neighbors_cold.p99_us,
        property_lookup_iters,
        property_lookup_index.rows_total,
        property_lookup_scan.avg_us,
        property_lookup_scan.p95_us,
        property_lookup_scan.p99_us,
        property_lookup_index.avg_us,
        property_lookup_index.p95_us,
        property_lookup_index.p99_us,
        property_lookup_speedup,
        write_txn.avg_us,
        write_txn.p95_us,
        write_txn.p99_us,
        write_txn_p99_ms,
        read_query_p99_ms
    );
}

fn bench_insert(db: &Db, cfg: Config) -> InsertBenchResult {
    let mut tx = db.begin_write();
    let stage_get_schema_start = Instant::now();
    let label = tx.get_or_create_label("BenchNode").unwrap();
    let rel = tx.get_or_create_rel_type("BENCH_EDGE").unwrap();
    let stage_get_schema_ms = elapsed_ms(stage_get_schema_start);

    let mut nodes = Vec::with_capacity(cfg.nodes);
    let stage_create_nodes_start = Instant::now();
    for i in 0..cfg.nodes {
        let external_id = (i as u64) + 1;
        let node = tx.create_node(external_id, label).unwrap();
        tx.set_node_property(
            node,
            "name".to_string(),
            PropertyValue::String(format!("node_{i}")),
        )
        .unwrap();
        nodes.push(node);
    }
    let stage_create_nodes_ms = elapsed_ms(stage_create_nodes_start);

    let stage_create_edges_start = Instant::now();
    for src_idx in 0..cfg.nodes {
        let src = nodes[src_idx];
        for j in 0..cfg.degree {
            let dst_idx = (src_idx + j + 1) % cfg.nodes;
            let dst = nodes[dst_idx];
            tx.create_edge(src, rel, dst).unwrap();
        }
    }
    let stage_create_edges_ms = elapsed_ms(stage_create_edges_start);

    let stage_commit_start = Instant::now();
    tx.commit().unwrap();
    let stage_commit_ms = elapsed_ms(stage_commit_start);

    InsertBenchResult {
        nodes,
        label,
        rel,
        stage_get_schema_ms,
        stage_create_nodes_ms,
        stage_create_edges_ms,
        stage_commit_ms,
    }
}

fn bench_neighbors_hot(db: &Db, src: u32, rel: u32, iters: usize) -> NeighborBenchResult {
    let snap = db.snapshot();

    let mut latencies_us = Vec::with_capacity(iters);
    let start = Instant::now();
    let mut edges_total: u64 = 0;
    for _ in 0..iters {
        let t0 = Instant::now();
        edges_total += snap.neighbors(src, Some(rel)).count() as u64;
        latencies_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let secs = start.elapsed().as_secs_f64().max(1e-9);
    summarize_neighbor_bench(edges_total, secs, latencies_us)
}

fn bench_neighbors_cold(db: &Db, nodes: &[u32], rel: u32, iters: usize) -> NeighborBenchResult {
    let snap = db.snapshot();
    let mut rng = SplitMix64::new(0x243f_6a88_85a3_08d3);

    let mut latencies_us = Vec::with_capacity(iters);
    let start = Instant::now();
    let mut edges_total: u64 = 0;
    for _ in 0..iters {
        let idx = (rng.next_u64() as usize) % nodes.len();
        let t0 = Instant::now();
        edges_total += snap.neighbors(nodes[idx], Some(rel)).count() as u64;
        latencies_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let secs = start.elapsed().as_secs_f64().max(1e-9);
    summarize_neighbor_bench(edges_total, secs, latencies_us)
}

fn bench_property_lookup_scan(
    db: &Db,
    label: u32,
    target_name: &str,
    iters: usize,
) -> PropertyLookupBenchResult {
    let snap = db.snapshot();
    let target = PropertyValue::String(target_name.to_string());
    let mut latencies_us = Vec::with_capacity(iters);
    let mut rows_total: u64 = 0;
    for _ in 0..iters {
        let t0 = Instant::now();
        let mut rows = 0u64;
        for node in snap.nodes_with_label(label) {
            if snap.node_property(node, "name").as_ref() == Some(&target) {
                rows += 1;
                break;
            }
        }
        latencies_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
        rows_total += rows;
    }
    summarize_property_lookup_bench(rows_total, latencies_us)
}

fn bench_property_lookup_index(
    db: &Db,
    label: u32,
    target_name: &str,
    iters: usize,
) -> PropertyLookupBenchResult {
    let snap = db.snapshot();
    let target = PropertyValue::String(target_name.to_string());
    let mut latencies_us = Vec::with_capacity(iters);
    let mut rows_total: u64 = 0;
    for _ in 0..iters {
        let t0 = Instant::now();
        let rows = snap
            .nodes_with_label_and_property(label, "name", &target)
            .take(1)
            .count() as u64;
        latencies_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
        rows_total += rows;
    }
    summarize_property_lookup_bench(rows_total, latencies_us)
}

fn bench_write_txn(
    db: &Db,
    label: u32,
    start_external_id: u64,
    write_iters: usize,
) -> WriteTxnBenchResult {
    let mut latencies_us = Vec::with_capacity(write_iters);
    for i in 0..write_iters {
        let t0 = Instant::now();
        let mut tx = db.begin_write();
        tx.create_node(start_external_id + i as u64, label).unwrap();
        tx.commit().unwrap();
        latencies_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }
    summarize_write_txn_bench(latencies_us)
}

#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut z = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        self.state = z;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

//! v2 micro-bench suite (M1/M2).
//!
//! Run:
//!   cargo run --example bench_v2 -p nervusdb-v2-storage --release -- --nodes 100000 --degree 16 --iters 2000

use nervusdb_v2_storage::engine::GraphEngine;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::tempdir;

#[derive(Debug, Clone)]
struct Config {
    nodes: usize,
    degree: usize,
    iters: usize,
    rel: u32,
    label: u32,
}

#[derive(Debug, Clone, Copy)]
struct NeighborBenchResult {
    edges_per_sec: f64,
    edges_total: u64,
    avg_us: f64,
    p95_us: f64,
    p99_us: f64,
}

impl Config {
    fn from_args() -> Self {
        let mut cfg = Self {
            nodes: 50_000,
            degree: 8,
            iters: 2_000,
            rel: 1,
            label: 1,
        };

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--nodes" => cfg.nodes = parse_usize(args.next()),
                "--degree" => cfg.degree = parse_usize(args.next()),
                "--iters" => cfg.iters = parse_usize(args.next()),
                "--rel" => cfg.rel = parse_u32(args.next()),
                "--label" => cfg.label = parse_u32(args.next()),
                _ => {
                    eprintln!(
                        "unknown arg: {arg}\n  supported: --nodes N --degree D --iters I --rel R --label L"
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

fn parse_u32(v: Option<String>) -> u32 {
    v.unwrap_or_else(|| {
        eprintln!("missing value");
        std::process::exit(2);
    })
    .parse::<u32>()
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

fn main() {
    let cfg = Config::from_args();

    let dir = tempdir().unwrap();
    let ndb = dir.path().join("bench.ndb");
    let wal = dir.path().join("bench.wal");

    let engine = GraphEngine::open(&ndb, &wal).unwrap();

    let total_edges = cfg.nodes * cfg.degree;
    let (nodes, insert_secs) = bench_insert(&engine, cfg.clone());
    let wal_bytes = file_len(&wal);

    let insert_edges_per_sec = total_edges as f64 / insert_secs.max(1e-9);
    let wal_bytes_per_edge = wal_bytes as f64 / total_edges.max(1) as f64;

    let m1_hot = bench_neighbors_hot(&engine, &nodes, cfg.rel, cfg.iters);
    let m1_cold = bench_neighbors_cold(&engine, &nodes, cfg.rel, cfg.iters);

    let compact_start = Instant::now();
    engine.compact().unwrap();
    let compact_secs = compact_start.elapsed().as_secs_f64();

    let m2_hot = bench_neighbors_hot(&engine, &nodes, cfg.rel, cfg.iters);
    let m2_cold = bench_neighbors_cold(&engine, &nodes, cfg.rel, cfg.iters);

    println!("=== NervusDB v2 Bench (M1/M2) ===");
    println!(
        "nodes={} degree={} edges={} iters={}",
        cfg.nodes, cfg.degree, total_edges, cfg.iters
    );
    println!(
        "insert: {:.3}s ({:.0} edges/sec), wal={:.2} B/edge",
        insert_secs, insert_edges_per_sec, wal_bytes_per_edge
    );
    println!(
        "neighbors_hot: M1 {:.0} edges/sec ({} edges, p95={:.2}us, p99={:.2}us), M2 {:.0} edges/sec ({} edges, p95={:.2}us, p99={:.2}us)",
        m1_hot.edges_per_sec,
        m1_hot.edges_total,
        m1_hot.p95_us,
        m1_hot.p99_us,
        m2_hot.edges_per_sec,
        m2_hot.edges_total,
        m2_hot.p95_us,
        m2_hot.p99_us
    );
    println!(
        "neighbors_cold: M1 {:.0} edges/sec ({} edges, p95={:.2}us, p99={:.2}us), M2 {:.0} edges/sec ({} edges, p95={:.2}us, p99={:.2}us)",
        m1_cold.edges_per_sec,
        m1_cold.edges_total,
        m1_cold.p95_us,
        m1_cold.p99_us,
        m2_cold.edges_per_sec,
        m2_cold.edges_total,
        m2_cold.p95_us,
        m2_cold.p99_us
    );
    println!("compact: {:.3}s", compact_secs);

    println!(
        "{{\"nodes\":{},\"degree\":{},\"edges\":{},\"iters\":{},\"insert_edges_per_sec\":{:.3},\"wal_bytes_per_edge\":{:.3},\"neighbors_hot_m1_edges_per_sec\":{:.3},\"neighbors_hot_m2_edges_per_sec\":{:.3},\"neighbors_cold_m1_edges_per_sec\":{:.3},\"neighbors_cold_m2_edges_per_sec\":{:.3},\"neighbors_hot_m1_p95_us\":{:.3},\"neighbors_hot_m1_p99_us\":{:.3},\"neighbors_hot_m2_p95_us\":{:.3},\"neighbors_hot_m2_p99_us\":{:.3},\"neighbors_cold_m1_p95_us\":{:.3},\"neighbors_cold_m1_p99_us\":{:.3},\"neighbors_cold_m2_p95_us\":{:.3},\"neighbors_cold_m2_p99_us\":{:.3},\"neighbors_hot_m2_avg_us\":{:.3},\"neighbors_cold_m2_avg_us\":{:.3},\"compact_secs\":{:.6}}}",
        cfg.nodes,
        cfg.degree,
        total_edges,
        cfg.iters,
        insert_edges_per_sec,
        wal_bytes_per_edge,
        m1_hot.edges_per_sec,
        m2_hot.edges_per_sec,
        m1_cold.edges_per_sec,
        m2_cold.edges_per_sec,
        m1_hot.p95_us,
        m1_hot.p99_us,
        m2_hot.p95_us,
        m2_hot.p99_us,
        m1_cold.p95_us,
        m1_cold.p99_us,
        m2_cold.p95_us,
        m2_cold.p99_us,
        m2_hot.avg_us,
        m2_cold.avg_us,
        compact_secs
    );
}

fn bench_insert(engine: &GraphEngine, cfg: Config) -> (Vec<u32>, f64) {
    let start = Instant::now();
    let mut tx = engine.begin_write();

    let mut nodes = Vec::with_capacity(cfg.nodes);
    for i in 0..cfg.nodes {
        let external_id = (i as u64) + 1;
        nodes.push(tx.create_node(external_id, cfg.label).unwrap());
    }

    for src_idx in 0..cfg.nodes {
        let src = nodes[src_idx];
        for j in 0..cfg.degree {
            let dst_idx = (src_idx + j + 1) % cfg.nodes;
            let dst = nodes[dst_idx];
            tx.create_edge(src, cfg.rel, dst);
        }
    }

    tx.commit().unwrap();

    (nodes, start.elapsed().as_secs_f64())
}

fn bench_neighbors_hot(
    engine: &GraphEngine,
    nodes: &[u32],
    rel: u32,
    iters: usize,
) -> NeighborBenchResult {
    let src = nodes[0];
    let snap = engine.begin_read();

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

fn bench_neighbors_cold(
    engine: &GraphEngine,
    nodes: &[u32],
    rel: u32,
    iters: usize,
) -> NeighborBenchResult {
    let snap = engine.begin_read();
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

fn file_len(path: &PathBuf) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
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

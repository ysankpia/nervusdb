//! HNSW tuning benchmark for NervusDB v2 storage.
//!
//! Example:
//! cargo run --example hnsw_tune -p nervusdb-storage --release -- \
//!   --nodes 2000 --dim 16 --queries 100 --k 10 --m 16 --ef-construction 200 --ef-search 200

use nervusdb_storage::engine::GraphEngine;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::tempdir;

#[derive(Debug, Clone)]
struct Config {
    nodes: usize,
    dim: usize,
    queries: usize,
    k: usize,
    m: usize,
    ef_construction: usize,
    ef_search: usize,
    label: u32,
}

#[derive(Debug, Clone, Copy)]
struct SearchStats {
    avg_us: f64,
    p95_us: f64,
    p99_us: f64,
    recall_at_k: f64,
}

impl Config {
    fn from_args() -> Self {
        let mut cfg = Self {
            nodes: 2_000,
            dim: 16,
            queries: 100,
            k: 10,
            m: 16,
            ef_construction: 200,
            ef_search: 200,
            label: 1,
        };

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--nodes" => cfg.nodes = parse_usize(args.next()),
                "--dim" => cfg.dim = parse_usize(args.next()),
                "--queries" => cfg.queries = parse_usize(args.next()),
                "--k" => cfg.k = parse_usize(args.next()),
                "--m" => cfg.m = parse_usize(args.next()),
                "--ef-construction" => cfg.ef_construction = parse_usize(args.next()),
                "--ef-search" => cfg.ef_search = parse_usize(args.next()),
                "--label" => cfg.label = parse_u32(args.next()),
                _ => {
                    eprintln!(
                        "unknown arg: {arg}\n  supported: --nodes N --dim D --queries Q --k K --m M --ef-construction EC --ef-search ES --label L"
                    );
                    std::process::exit(2);
                }
            }
        }

        if cfg.nodes == 0 || cfg.dim == 0 || cfg.queries == 0 || cfg.k == 0 {
            eprintln!("nodes/dim/queries/k must be > 0");
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

fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum::<f32>()
        .sqrt()
}

fn brute_force_topk(vectors: &[Vec<f32>], query: &[f32], k: usize) -> Vec<usize> {
    let mut scored: Vec<(usize, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(idx, v)| (idx, l2_distance(v, query)))
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(idx, _)| idx).collect()
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

    fn next_f32(&mut self) -> f32 {
        let v = self.next_u64();
        (v as f64 / u64::MAX as f64) as f32
    }
}

fn make_vectors(nodes: usize, dim: usize) -> Vec<Vec<f32>> {
    let mut rng = SplitMix64::new(0xdead_beef_cafe_babe);
    let mut out = Vec::with_capacity(nodes);
    for _ in 0..nodes {
        let mut v = Vec::with_capacity(dim);
        for _ in 0..dim {
            v.push(rng.next_f32());
        }
        out.push(v);
    }
    out
}

fn run_search_eval(
    engine: &GraphEngine,
    ids: &[u32],
    vectors: &[Vec<f32>],
    cfg: &Config,
) -> SearchStats {
    let mut rng = SplitMix64::new(0x1234_5678_9abc_def0);

    let mut total_recall = 0.0;
    let mut latencies_us = Vec::with_capacity(cfg.queries);

    for _ in 0..cfg.queries {
        let base_idx = (rng.next_u64() as usize) % vectors.len();
        let mut query = vectors[base_idx].clone();
        for item in &mut query {
            let noise = (rng.next_f32() - 0.5) * 0.01;
            *item += noise;
        }

        let expected = brute_force_topk(vectors, &query, cfg.k.min(vectors.len()));
        let expected_ids: HashSet<u32> = expected.into_iter().map(|idx| ids[idx]).collect();

        let t0 = Instant::now();
        let actual = engine
            .search_vector(&query, cfg.k.min(ids.len()))
            .expect("hnsw search should succeed");
        latencies_us.push(t0.elapsed().as_secs_f64() * 1_000_000.0);

        let actual_ids: HashSet<u32> = actual.into_iter().map(|(id, _)| id).collect();
        let hit_count = actual_ids.intersection(&expected_ids).count();
        total_recall += hit_count as f64 / cfg.k.min(ids.len()) as f64;
    }

    let avg_us = if latencies_us.is_empty() {
        0.0
    } else {
        latencies_us.iter().sum::<f64>() / latencies_us.len() as f64
    };

    SearchStats {
        avg_us,
        p95_us: percentile_us(latencies_us.clone(), 0.95),
        p99_us: percentile_us(latencies_us, 0.99),
        recall_at_k: total_recall / cfg.queries as f64,
    }
}

fn with_hnsw_env<T>(cfg: &Config, f: impl FnOnce() -> T) -> T {
    let old_m = std::env::var("NERVUSDB_HNSW_M").ok();
    let old_ec = std::env::var("NERVUSDB_HNSW_EF_CONSTRUCTION").ok();
    let old_es = std::env::var("NERVUSDB_HNSW_EF_SEARCH").ok();

    unsafe { std::env::set_var("NERVUSDB_HNSW_M", cfg.m.to_string()) };
    unsafe {
        std::env::set_var(
            "NERVUSDB_HNSW_EF_CONSTRUCTION",
            cfg.ef_construction.to_string(),
        )
    };
    unsafe { std::env::set_var("NERVUSDB_HNSW_EF_SEARCH", cfg.ef_search.to_string()) };

    let out = f();

    if let Some(v) = old_m {
        unsafe { std::env::set_var("NERVUSDB_HNSW_M", v) };
    } else {
        unsafe { std::env::remove_var("NERVUSDB_HNSW_M") };
    }
    if let Some(v) = old_ec {
        unsafe { std::env::set_var("NERVUSDB_HNSW_EF_CONSTRUCTION", v) };
    } else {
        unsafe { std::env::remove_var("NERVUSDB_HNSW_EF_CONSTRUCTION") };
    }
    if let Some(v) = old_es {
        unsafe { std::env::set_var("NERVUSDB_HNSW_EF_SEARCH", v) };
    } else {
        unsafe { std::env::remove_var("NERVUSDB_HNSW_EF_SEARCH") };
    }

    out
}

fn main() {
    let cfg = Config::from_args();
    let vectors = make_vectors(cfg.nodes, cfg.dim);

    let dir = tempdir().expect("tempdir should be created");
    let ndb = dir.path().join("hnsw_tune.ndb");
    let wal = dir.path().join("hnsw_tune.wal");

    let (stats, ndb_bytes, wal_bytes) = with_hnsw_env(&cfg, || {
        let engine = GraphEngine::open(&ndb, &wal).expect("engine open should succeed");

        let mut ids = Vec::with_capacity(cfg.nodes);
        {
            let mut tx = engine.begin_write();
            for (idx, vector) in vectors.iter().enumerate() {
                let node_id = tx
                    .create_node((idx as u64) + 1, cfg.label)
                    .expect("create node should succeed");
                tx.set_vector(node_id, vector.clone())
                    .expect("set vector should succeed");
                ids.push(node_id);
            }
            tx.commit().expect("commit should succeed");
        }

        let stats = run_search_eval(&engine, &ids, &vectors, &cfg);
        let ndb_bytes = file_len(&ndb);
        let wal_bytes = file_len(&wal);
        (stats, ndb_bytes, wal_bytes)
    });

    let total_bytes = ndb_bytes + wal_bytes;

    println!(
        "{{\"nodes\":{},\"dim\":{},\"queries\":{},\"k\":{},\"m\":{},\"ef_construction\":{},\"ef_search\":{},\"recall_at_k\":{:.6},\"avg_us\":{:.3},\"p95_us\":{:.3},\"p99_us\":{:.3},\"ndb_bytes\":{},\"wal_bytes\":{},\"memory_proxy_bytes\":{}}}",
        cfg.nodes,
        cfg.dim,
        cfg.queries,
        cfg.k,
        cfg.m,
        cfg.ef_construction,
        cfg.ef_search,
        stats.recall_at_k,
        stats.avg_us,
        stats.p95_us,
        stats.p99_us,
        ndb_bytes,
        wal_bytes,
        total_bytes
    );
}

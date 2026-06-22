//! Cross-database embedded graph benchmark, phase 1.
//!
//! This is research evidence, not a public API example. It compares the
//! released NervusDB facade against two SQLite graph schemas using the same
//! generated property-graph workload.

use nervusdb::{Db, GraphSnapshot, PropertyValue};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::cmp::min;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::{TempDir, tempdir};

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

const BENCHMARK_VERSION: u32 = 1;
const LABEL_NAME: &str = "BenchNode";
const REL_NAME: &str = "LINK";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    NervusDb,
    SqliteSimple,
    SqliteMaterialized,
}

impl System {
    fn parse(value: &str) -> Self {
        match value {
            "nervusdb" => Self::NervusDb,
            "sqlite-simple" => Self::SqliteSimple,
            "sqlite-materialized" => Self::SqliteMaterialized,
            _ => {
                eprintln!(
                    "unknown system: {value}\n  supported: nervusdb | sqlite-simple | sqlite-materialized"
                );
                std::process::exit(2);
            }
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::NervusDb => "nervusdb",
            Self::SqliteSimple => "sqlite-simple",
            Self::SqliteMaterialized => "sqlite-materialized",
        }
    }
}

#[derive(Debug, Clone)]
struct Config {
    system: System,
    nodes: usize,
    degree: usize,
    iters: usize,
    mutation_iters: usize,
    seed: u64,
}

impl Config {
    fn from_args() -> Self {
        let mut system = System::NervusDb;
        let mut nodes = 10_000;
        let mut degree = 5;
        let mut iters = 1_000;
        let mut mutation_iters: Option<usize> = None;
        let mut seed = 1;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--system" => system = System::parse(&required_value(args.next(), "--system")),
                "--nodes" => nodes = parse_usize(args.next(), "--nodes"),
                "--degree" => degree = parse_usize(args.next(), "--degree"),
                "--iters" => iters = parse_usize(args.next(), "--iters"),
                "--mutation-iters" => {
                    mutation_iters = Some(parse_usize(args.next(), "--mutation-iters"));
                }
                "--seed" => seed = parse_u64(args.next(), "--seed"),
                _ => {
                    eprintln!(
                        "unknown arg: {arg}\n  supported: --system S --nodes N --degree D --iters I --mutation-iters M --seed SEED"
                    );
                    std::process::exit(2);
                }
            }
        }

        if nodes < 2 {
            eprintln!("--nodes must be >= 2");
            std::process::exit(2);
        }
        if degree == 0 || degree >= nodes {
            eprintln!("--degree must be > 0 and < nodes");
            std::process::exit(2);
        }
        if iters == 0 {
            eprintln!("--iters must be > 0");
            std::process::exit(2);
        }
        let mutation_iters = mutation_iters.unwrap_or_else(|| min(100, iters));
        if mutation_iters == 0 {
            eprintln!("--mutation-iters must be > 0");
            std::process::exit(2);
        }

        Self {
            system,
            nodes,
            degree,
            iters,
            mutation_iters,
            seed,
        }
    }

    fn edge_count(&self) -> usize {
        self.nodes * self.degree
    }

    fn lookup_target_id(&self) -> u64 {
        (self.nodes / 2 + 1) as u64
    }

    fn lookup_target_name(&self) -> String {
        format!("node_{}", self.lookup_target_id() - 1)
    }

    fn mutation_count(&self) -> usize {
        min(self.mutation_iters, (self.nodes / 10).max(1))
    }

    fn update_ids(&self) -> Vec<u64> {
        (1..=self.mutation_count() as u64).collect()
    }

    fn delete_ids(&self) -> Vec<u64> {
        let count = self.mutation_count() as u64;
        ((self.nodes as u64 - count + 1)..=self.nodes as u64).collect()
    }
}

#[derive(Debug, Clone, Copy)]
struct LatencySummary {
    avg_us: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
}

impl LatencySummary {
    fn from_samples(samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self {
                avg_us: 0.0,
                p50_us: 0.0,
                p95_us: 0.0,
                p99_us: 0.0,
            };
        }
        let avg_us = samples.iter().sum::<f64>() / samples.len() as f64;
        Self {
            avg_us,
            p50_us: percentile_us(samples.clone(), 0.50),
            p95_us: percentile_us(samples.clone(), 0.95),
            p99_us: percentile_us(samples, 0.99),
        }
    }
}

#[derive(Debug, Clone)]
struct RunMetrics {
    system_version: String,
    storage_path: PathBuf,
    load_nodes_ms: f64,
    load_edges_ms: f64,
    commit_ms: f64,
    reopen_verify_ms: f64,
    lookup: LatencySummary,
    lookup_rows_total: u64,
    one_hop_hot_edges_per_sec: f64,
    one_hop_cold_edges_per_sec: f64,
    incoming_cold_edges_per_sec: f64,
    two_hop_paths_per_sec: f64,
    update: LatencySummary,
    detach_delete: LatencySummary,
    db_bytes: u64,
    db_file_count: u64,
    correctness_hash: String,
    notes: Vec<String>,
}

fn main() -> AnyResult<()> {
    let cfg = Config::from_args();

    let temp = tempdir()?;
    let metrics = match cfg.system {
        System::NervusDb => run_nervusdb(&cfg, &temp)?,
        System::SqliteSimple => run_sqlite_simple(&cfg, &temp)?,
        System::SqliteMaterialized => run_sqlite_materialized(&cfg, &temp)?,
    };

    println!("=== Cross DB Embedded Graph Bench ===");
    println!(
        "system={} nodes={} degree={} edges={} iters={} mutation_iters={} seed={}",
        cfg.system.as_str(),
        cfg.nodes,
        cfg.degree,
        cfg.edge_count(),
        cfg.iters,
        cfg.mutation_count(),
        cfg.seed
    );
    println!(
        "load: nodes={:.2}ms edges={:.2}ms commit={:.2}ms reopen={:.2}ms",
        metrics.load_nodes_ms, metrics.load_edges_ms, metrics.commit_ms, metrics.reopen_verify_ms
    );
    println!(
        "lookup: avg={:.2}us p50={:.2}us p95={:.2}us p99={:.2}us rows={}",
        metrics.lookup.avg_us,
        metrics.lookup.p50_us,
        metrics.lookup.p95_us,
        metrics.lookup.p99_us,
        metrics.lookup_rows_total
    );
    println!(
        "traversal: hot={:.0} out_edges/s cold={:.0} out_edges/s incoming={:.0} in_edges/s two_hop={:.0} paths/s",
        metrics.one_hop_hot_edges_per_sec,
        metrics.one_hop_cold_edges_per_sec,
        metrics.incoming_cold_edges_per_sec,
        metrics.two_hop_paths_per_sec
    );
    println!(
        "mutation: update_p99={:.2}us detach_delete_p99={:.2}us disk={} bytes files={}",
        metrics.update.p99_us,
        metrics.detach_delete.p99_us,
        metrics.db_bytes,
        metrics.db_file_count
    );
    println!("correctness_hash={}", metrics.correctness_hash);

    println!(
        "{}",
        json!({
            "benchmark_version": BENCHMARK_VERSION,
            "system": cfg.system.as_str(),
            "system_version": metrics.system_version,
            "profile": "safe",
            "load_mode": "single_transaction",
            "dataset": "custom",
            "shape": "uniform_degree",
            "seed": cfg.seed,
            "nodes": cfg.nodes,
            "degree": cfg.degree,
            "edges": cfg.edge_count(),
            "iters": cfg.iters,
            "mutation_iters": cfg.mutation_count(),
            "load_nodes_ms": round3(metrics.load_nodes_ms),
            "load_edges_ms": round3(metrics.load_edges_ms),
            "commit_ms": round3(metrics.commit_ms),
            "reopen_verify_ms": round3(metrics.reopen_verify_ms),
            "lookup_avg_us": round3(metrics.lookup.avg_us),
            "lookup_p50_us": round3(metrics.lookup.p50_us),
            "lookup_p95_us": round3(metrics.lookup.p95_us),
            "lookup_p99_us": round3(metrics.lookup.p99_us),
            "lookup_rows_total": metrics.lookup_rows_total,
            "one_hop_hot_edges_per_sec": round3(metrics.one_hop_hot_edges_per_sec),
            "one_hop_cold_edges_per_sec": round3(metrics.one_hop_cold_edges_per_sec),
            "incoming_cold_edges_per_sec": round3(metrics.incoming_cold_edges_per_sec),
            "two_hop_paths_per_sec": round3(metrics.two_hop_paths_per_sec),
            "update_avg_us": round3(metrics.update.avg_us),
            "update_p50_us": round3(metrics.update.p50_us),
            "update_p95_us": round3(metrics.update.p95_us),
            "update_p99_us": round3(metrics.update.p99_us),
            "detach_delete_avg_us": round3(metrics.detach_delete.avg_us),
            "detach_delete_p50_us": round3(metrics.detach_delete.p50_us),
            "detach_delete_p95_us": round3(metrics.detach_delete.p95_us),
            "detach_delete_p99_us": round3(metrics.detach_delete.p99_us),
            "db_bytes": metrics.db_bytes,
            "db_file_count": metrics.db_file_count,
            "correctness_hash": metrics.correctness_hash,
            "storage_path": metrics.storage_path.display().to_string(),
            "notes": metrics.notes,
        })
    );

    Ok(())
}

fn run_nervusdb(cfg: &Config, temp: &TempDir) -> AnyResult<RunMetrics> {
    let path = temp.path().join("nervusdb");
    let db = Db::open(&path)?;

    let (node_ids, label, rel, load_nodes_ms, load_edges_ms, commit_ms) = load_nervusdb(&db, cfg)?;

    let reopen_start = Instant::now();
    db.close()?;
    let db = Db::open(&path)?;
    {
        let snapshot = db.snapshot();
        assert_eq!(snapshot.node_count(Some(label)), cfg.nodes as u64);
        assert_eq!(snapshot.edge_count(Some(rel)), cfg.edge_count() as u64);
    }
    let reopen_verify_ms = elapsed_ms(reopen_start);

    let lookup = bench_nervusdb_lookup(&db, label, &cfg.lookup_target_name(), cfg.iters);
    let hot = bench_nervusdb_outgoing_hot(&db, node_ids[0], rel, cfg.iters);
    let cold = bench_nervusdb_outgoing_cold(&db, &node_ids, rel, cfg.iters, cfg.seed);
    let incoming = bench_nervusdb_incoming_cold(&db, &node_ids, rel, cfg.iters, cfg.seed);
    let two_hop = bench_nervusdb_two_hop(&db, &node_ids, rel, cfg.iters, cfg.seed);
    let update = bench_nervusdb_updates(&db, &node_ids, cfg);
    let detach_delete = bench_nervusdb_deletes(&db, &node_ids, cfg);
    let correctness_hash = nervusdb_correctness_hash(&db, cfg, label, rel, &node_ids);

    db.close()?;
    let (db_bytes, db_file_count) = path_stats(&path)?;

    Ok(RunMetrics {
        system_version: env!("CARGO_PKG_VERSION").to_string(),
        storage_path: path,
        load_nodes_ms,
        load_edges_ms,
        commit_ms,
        reopen_verify_ms,
        lookup_rows_total: lookup.1,
        lookup: lookup.0,
        one_hop_hot_edges_per_sec: hot,
        one_hop_cold_edges_per_sec: cold,
        incoming_cold_edges_per_sec: incoming,
        two_hop_paths_per_sec: two_hop,
        update,
        detach_delete,
        db_bytes,
        db_file_count,
        correctness_hash,
        notes: vec!["NervusDB public Rust facade".to_string()],
    })
}

fn load_nervusdb(db: &Db, cfg: &Config) -> AnyResult<(Vec<u32>, u32, u32, f64, f64, f64)> {
    let mut tx = db.begin_write();
    let label = tx.get_or_create_label(LABEL_NAME)?;
    let rel = tx.get_or_create_rel_type(REL_NAME)?;

    let mut node_ids = Vec::with_capacity(cfg.nodes);
    let load_nodes_start = Instant::now();
    for i in 0..cfg.nodes {
        let node = tx.create_node((i + 1) as u64, label)?;
        tx.set_node_property(
            node,
            "name".to_string(),
            PropertyValue::String(node_name(i)),
        )?;
        tx.set_node_property(
            node,
            "kind".to_string(),
            PropertyValue::String(node_kind(i)),
        )?;
        tx.set_node_property(
            node,
            "status".to_string(),
            PropertyValue::String("active".to_string()),
        )?;
        tx.set_node_property(
            node,
            "chapter".to_string(),
            PropertyValue::Int((i % 64) as i64),
        )?;
        node_ids.push(node);
    }
    let load_nodes_ms = elapsed_ms(load_nodes_start);

    let load_edges_start = Instant::now();
    for (src_idx, dst_idx) in generated_edges(cfg) {
        tx.create_edge(node_ids[src_idx], rel, node_ids[dst_idx])?;
    }
    let load_edges_ms = elapsed_ms(load_edges_start);

    let commit_start = Instant::now();
    tx.commit()?;
    let commit_ms = elapsed_ms(commit_start);

    Ok((
        node_ids,
        label,
        rel,
        load_nodes_ms,
        load_edges_ms,
        commit_ms,
    ))
}

fn bench_nervusdb_lookup(
    db: &Db,
    label: u32,
    target_name: &str,
    iters: usize,
) -> (LatencySummary, u64) {
    let snapshot = db.snapshot();
    let target = PropertyValue::String(target_name.to_string());
    let mut samples = Vec::with_capacity(iters);
    let mut rows_total = 0;
    for _ in 0..iters {
        let start = Instant::now();
        let rows = snapshot
            .nodes_with_label_and_property(label, "name", &target)
            .take(1)
            .count() as u64;
        samples.push(elapsed_us(start));
        rows_total += rows;
    }
    (LatencySummary::from_samples(samples), rows_total)
}

fn bench_nervusdb_outgoing_hot(db: &Db, src: u32, rel: u32, iters: usize) -> f64 {
    let snapshot = db.snapshot();
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..iters {
        edges_total += snapshot.neighbors(src, Some(rel)).count() as u64;
    }
    edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9)
}

fn bench_nervusdb_outgoing_cold(db: &Db, nodes: &[u32], rel: u32, iters: usize, seed: u64) -> f64 {
    let snapshot = db.snapshot();
    let mut rng = SplitMix64::new(seed ^ 0x1111_2222_3333_4444);
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..iters {
        let node = nodes[(rng.next_u64() as usize) % nodes.len()];
        edges_total += snapshot.neighbors(node, Some(rel)).count() as u64;
    }
    edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9)
}

fn bench_nervusdb_incoming_cold(db: &Db, nodes: &[u32], rel: u32, iters: usize, seed: u64) -> f64 {
    let snapshot = db.snapshot();
    let mut rng = SplitMix64::new(seed ^ 0x5555_6666_7777_8888);
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..iters {
        let node = nodes[(rng.next_u64() as usize) % nodes.len()];
        edges_total += snapshot.incoming_neighbors(node, Some(rel)).count() as u64;
    }
    edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9)
}

fn bench_nervusdb_two_hop(db: &Db, nodes: &[u32], rel: u32, iters: usize, seed: u64) -> f64 {
    let snapshot = db.snapshot();
    let mut rng = SplitMix64::new(seed ^ 0x9999_aaaa_bbbb_cccc);
    let start = Instant::now();
    let mut paths_total = 0;
    for _ in 0..iters {
        let node = nodes[(rng.next_u64() as usize) % nodes.len()];
        for edge in snapshot.neighbors(node, Some(rel)) {
            paths_total += snapshot.neighbors(edge.dst, Some(rel)).count() as u64;
        }
    }
    paths_total as f64 / start.elapsed().as_secs_f64().max(1e-9)
}

fn bench_nervusdb_updates(db: &Db, nodes: &[u32], cfg: &Config) -> LatencySummary {
    let mut samples = Vec::with_capacity(cfg.mutation_count());
    for external_id in cfg.update_ids() {
        let node = nodes[(external_id - 1) as usize];
        let start = Instant::now();
        let mut tx = db.begin_write();
        tx.set_node_property(
            node,
            "status".to_string(),
            PropertyValue::String("updated".to_string()),
        )
        .unwrap();
        tx.commit().unwrap();
        samples.push(elapsed_us(start));
    }
    LatencySummary::from_samples(samples)
}

fn bench_nervusdb_deletes(db: &Db, nodes: &[u32], cfg: &Config) -> LatencySummary {
    let mut samples = Vec::with_capacity(cfg.mutation_count());
    for external_id in cfg.delete_ids() {
        let node = nodes[(external_id - 1) as usize];
        let start = Instant::now();
        let mut tx = db.begin_write();
        tx.tombstone_node(node).unwrap();
        tx.commit().unwrap();
        samples.push(elapsed_us(start));
    }
    LatencySummary::from_samples(samples)
}

fn nervusdb_correctness_hash(db: &Db, cfg: &Config, label: u32, rel: u32, nodes: &[u32]) -> String {
    let snapshot = db.snapshot();
    let sample_mid = nodes[cfg.nodes / 2];
    let target = PropertyValue::String(cfg.lookup_target_name());
    let status = PropertyValue::String("updated".to_string());
    let mut parts = Vec::new();
    parts.push(format!("node_count={}", snapshot.node_count(Some(label))));
    parts.push(format!("edge_count={}", snapshot.edge_count(Some(rel))));
    let lookup_ids: Vec<_> = snapshot
        .nodes_with_label_and_property(label, "name", &target)
        .filter_map(|iid| snapshot.resolve_external(iid))
        .collect();
    parts.push(format!("lookup={lookup_ids:?}"));
    let updated_count = snapshot
        .nodes_with_label_and_property(label, "status", &status)
        .count();
    parts.push(format!("updated_count={updated_count}"));
    parts.push(format!(
        "out_mid={:?}",
        external_dsts(&snapshot, sample_mid, rel)
    ));
    parts.push(format!(
        "in_mid={:?}",
        external_srcs(&snapshot, sample_mid, rel)
    ));
    parts.push(format!(
        "two_hop_mid={}",
        two_hop_count_nervusdb(&snapshot, sample_mid, rel)
    ));
    for external_id in cfg.delete_ids() {
        let node = nodes[(external_id - 1) as usize];
        parts.push(format!(
            "deleted_{external_id}={}",
            snapshot.is_tombstoned_node(node)
        ));
    }
    stable_hash(&parts)
}

fn external_dsts(snapshot: &impl GraphSnapshot, node: u32, rel: u32) -> Vec<u64> {
    let mut out: Vec<_> = snapshot
        .neighbors(node, Some(rel))
        .filter_map(|edge| snapshot.resolve_external(edge.dst))
        .collect();
    out.sort_unstable();
    out
}

fn external_srcs(snapshot: &impl GraphSnapshot, node: u32, rel: u32) -> Vec<u64> {
    let mut out: Vec<_> = snapshot
        .incoming_neighbors(node, Some(rel))
        .filter_map(|edge| snapshot.resolve_external(edge.src))
        .collect();
    out.sort_unstable();
    out
}

fn two_hop_count_nervusdb(snapshot: &impl GraphSnapshot, node: u32, rel: u32) -> u64 {
    let mut count = 0;
    for edge in snapshot.neighbors(node, Some(rel)) {
        count += snapshot.neighbors(edge.dst, Some(rel)).count() as u64;
    }
    count
}

fn run_sqlite_simple(cfg: &Config, temp: &TempDir) -> AnyResult<RunMetrics> {
    let path = temp.path().join("sqlite-simple.db");
    let mut conn = open_sqlite(&path)?;
    create_sqlite_simple_schema(&conn)?;
    let (load_nodes_ms, load_edges_ms, commit_ms) = load_sqlite_simple(&mut conn, cfg)?;

    let reopen_start = Instant::now();
    drop(conn);
    let conn = open_sqlite(&path)?;
    assert_eq!(
        sqlite_count(
            &conn,
            "SELECT COUNT(*) FROM nodes WHERE label = 'BenchNode'"
        )?,
        cfg.nodes as u64
    );
    assert_eq!(
        sqlite_count(&conn, "SELECT COUNT(*) FROM edges WHERE rel = 'LINK'")?,
        cfg.edge_count() as u64
    );
    let reopen_verify_ms = elapsed_ms(reopen_start);

    let lookup = bench_sqlite_lookup_simple(&conn, cfg)?;
    let hot = bench_sqlite_one_hop_hot_simple(&conn, cfg)?;
    let cold = bench_sqlite_one_hop_cold_simple(&conn, cfg)?;
    let incoming = bench_sqlite_incoming_cold_simple(&conn, cfg)?;
    let two_hop = bench_sqlite_two_hop_simple(&conn, cfg)?;
    let update = bench_sqlite_updates_simple(&conn, cfg)?;
    let detach_delete = bench_sqlite_deletes_simple(&conn, cfg)?;
    let correctness_hash = sqlite_simple_correctness_hash(&conn, cfg)?;

    drop(conn);
    let (db_bytes, db_file_count) = path_stats(temp.path())?;

    Ok(RunMetrics {
        system_version: sqlite_version(&path)?,
        storage_path: path,
        load_nodes_ms,
        load_edges_ms,
        commit_ms,
        reopen_verify_ms,
        lookup: lookup.0,
        lookup_rows_total: lookup.1,
        one_hop_hot_edges_per_sec: hot,
        one_hop_cold_edges_per_sec: cold,
        incoming_cold_edges_per_sec: incoming,
        two_hop_paths_per_sec: two_hop,
        update,
        detach_delete,
        db_bytes,
        db_file_count,
        correctness_hash,
        notes: vec!["SQLite direct nodes/edges relational schema".to_string()],
    })
}

fn open_sqlite(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

fn create_sqlite_simple_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE nodes (
          id INTEGER PRIMARY KEY,
          label TEXT NOT NULL,
          name TEXT NOT NULL,
          kind TEXT NOT NULL,
          status TEXT NOT NULL,
          chapter INTEGER NOT NULL
        );
        CREATE INDEX nodes_label_name_idx ON nodes(label, name);
        CREATE INDEX nodes_label_status_idx ON nodes(label, status);

        CREATE TABLE edges (
          src INTEGER NOT NULL,
          rel TEXT NOT NULL,
          dst INTEGER NOT NULL,
          PRIMARY KEY (src, rel, dst),
          FOREIGN KEY (src) REFERENCES nodes(id) ON DELETE CASCADE,
          FOREIGN KEY (dst) REFERENCES nodes(id) ON DELETE CASCADE
        ) WITHOUT ROWID;
        CREATE INDEX edges_dst_rel_src_idx ON edges(dst, rel, src);
        ",
    )
}

fn load_sqlite_simple(conn: &mut Connection, cfg: &Config) -> AnyResult<(f64, f64, f64)> {
    let tx = conn.transaction()?;

    let nodes_start = Instant::now();
    {
        let mut stmt = tx.prepare(
            "INSERT INTO nodes(id, label, name, kind, status, chapter) VALUES (?, ?, ?, ?, ?, ?)",
        )?;
        for i in 0..cfg.nodes {
            stmt.execute(params![
                (i + 1) as i64,
                LABEL_NAME,
                node_name(i),
                node_kind(i),
                "active",
                (i % 64) as i64
            ])?;
        }
    }
    let load_nodes_ms = elapsed_ms(nodes_start);

    let edges_start = Instant::now();
    {
        let mut stmt = tx.prepare("INSERT INTO edges(src, rel, dst) VALUES (?, ?, ?)")?;
        for (src_idx, dst_idx) in generated_edges(cfg) {
            stmt.execute(params![
                (src_idx + 1) as i64,
                REL_NAME,
                (dst_idx + 1) as i64
            ])?;
        }
    }
    let load_edges_ms = elapsed_ms(edges_start);

    let commit_start = Instant::now();
    tx.commit()?;
    let commit_ms = elapsed_ms(commit_start);
    Ok((load_nodes_ms, load_edges_ms, commit_ms))
}

fn bench_sqlite_lookup_simple(conn: &Connection, cfg: &Config) -> AnyResult<(LatencySummary, u64)> {
    let mut stmt =
        conn.prepare("SELECT id FROM nodes WHERE label = 'BenchNode' AND name = ? LIMIT 1")?;
    let target = cfg.lookup_target_name();
    let mut samples = Vec::with_capacity(cfg.iters);
    let mut rows_total = 0;
    for _ in 0..cfg.iters {
        let start = Instant::now();
        let row: Option<i64> = stmt
            .query_row(params![target], |row| row.get(0))
            .optional()?;
        rows_total += u64::from(row.is_some());
        samples.push(elapsed_us(start));
    }
    Ok((LatencySummary::from_samples(samples), rows_total))
}

fn bench_sqlite_one_hop_hot_simple(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare("SELECT dst FROM edges WHERE src = ? AND rel = 'LINK'")?;
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..cfg.iters {
        edges_total += stmt.query_map(params![1_i64], |_| Ok(()))?.count() as u64;
    }
    Ok(edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_one_hop_cold_simple(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare("SELECT dst FROM edges WHERE src = ? AND rel = 'LINK'")?;
    let mut rng = SplitMix64::new(cfg.seed ^ 0x1111_2222_3333_4444);
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..cfg.iters {
        let src = random_node_id(&mut rng, cfg);
        edges_total += stmt.query_map(params![src], |_| Ok(()))?.count() as u64;
    }
    Ok(edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_incoming_cold_simple(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare("SELECT src FROM edges WHERE dst = ? AND rel = 'LINK'")?;
    let mut rng = SplitMix64::new(cfg.seed ^ 0x5555_6666_7777_8888);
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..cfg.iters {
        let dst = random_node_id(&mut rng, cfg);
        edges_total += stmt.query_map(params![dst], |_| Ok(()))?.count() as u64;
    }
    Ok(edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_two_hop_simple(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare(
        "SELECT e2.dst
         FROM edges e1
         JOIN edges e2 ON e2.src = e1.dst AND e2.rel = 'LINK'
         WHERE e1.src = ? AND e1.rel = 'LINK'",
    )?;
    let mut rng = SplitMix64::new(cfg.seed ^ 0x9999_aaaa_bbbb_cccc);
    let start = Instant::now();
    let mut paths_total = 0;
    for _ in 0..cfg.iters {
        let src = random_node_id(&mut rng, cfg);
        paths_total += stmt.query_map(params![src], |_| Ok(()))?.count() as u64;
    }
    Ok(paths_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_updates_simple(conn: &Connection, cfg: &Config) -> AnyResult<LatencySummary> {
    let mut samples = Vec::with_capacity(cfg.mutation_count());
    for id in cfg.update_ids() {
        let start = Instant::now();
        conn.execute("BEGIN IMMEDIATE", [])?;
        conn.execute(
            "UPDATE nodes SET status = 'updated' WHERE id = ?",
            params![id as i64],
        )?;
        conn.execute("COMMIT", [])?;
        samples.push(elapsed_us(start));
    }
    Ok(LatencySummary::from_samples(samples))
}

fn bench_sqlite_deletes_simple(conn: &Connection, cfg: &Config) -> AnyResult<LatencySummary> {
    let mut samples = Vec::with_capacity(cfg.mutation_count());
    for id in cfg.delete_ids() {
        let start = Instant::now();
        conn.execute("BEGIN IMMEDIATE", [])?;
        conn.execute("DELETE FROM nodes WHERE id = ?", params![id as i64])?;
        conn.execute("COMMIT", [])?;
        samples.push(elapsed_us(start));
    }
    Ok(LatencySummary::from_samples(samples))
}

fn sqlite_simple_correctness_hash(conn: &Connection, cfg: &Config) -> AnyResult<String> {
    let mut parts = Vec::new();
    parts.push(format!(
        "node_count={}",
        sqlite_count(conn, "SELECT COUNT(*) FROM nodes WHERE label = 'BenchNode'")?
    ));
    parts.push(format!(
        "edge_count={}",
        sqlite_count(conn, "SELECT COUNT(*) FROM edges WHERE rel = 'LINK'")?
    ));
    let lookup = collect_i64(
        conn,
        "SELECT id FROM nodes WHERE label = 'BenchNode' AND name = ? ORDER BY id",
        &[&cfg.lookup_target_name()],
    )?;
    parts.push(format!("lookup={lookup:?}"));
    parts.push(format!(
        "updated_count={}",
        sqlite_count(conn, "SELECT COUNT(*) FROM nodes WHERE status = 'updated'")?
    ));
    let mid = (cfg.nodes / 2 + 1) as i64;
    let out = collect_i64(
        conn,
        "SELECT dst FROM edges WHERE src = ? AND rel = 'LINK' ORDER BY dst",
        &[&mid],
    )?;
    parts.push(format!("out_mid={out:?}"));
    let incoming = collect_i64(
        conn,
        "SELECT src FROM edges WHERE dst = ? AND rel = 'LINK' ORDER BY src",
        &[&mid],
    )?;
    parts.push(format!("in_mid={incoming:?}"));
    parts.push(format!(
        "two_hop_mid={}",
        sqlite_count_param(
            conn,
            "SELECT COUNT(*)
             FROM edges e1
             JOIN edges e2 ON e2.src = e1.dst AND e2.rel = 'LINK'
             WHERE e1.src = ? AND e1.rel = 'LINK'",
            mid,
        )?
    ));
    for id in cfg.delete_ids() {
        let exists =
            sqlite_count_param(conn, "SELECT COUNT(*) FROM nodes WHERE id = ?", id as i64)?;
        parts.push(format!("deleted_{id}={}", exists == 0));
    }
    Ok(stable_hash(&parts))
}

fn run_sqlite_materialized(cfg: &Config, temp: &TempDir) -> AnyResult<RunMetrics> {
    let path = temp.path().join("sqlite-materialized.db");
    let mut conn = open_sqlite(&path)?;
    create_sqlite_materialized_schema(&conn)?;
    let (load_nodes_ms, load_edges_ms, commit_ms) = load_sqlite_materialized(&mut conn, cfg)?;

    let reopen_start = Instant::now();
    drop(conn);
    let conn = open_sqlite(&path)?;
    assert_eq!(
        sqlite_count(
            &conn,
            "SELECT COUNT(*)
             FROM node_labels
             WHERE label_id = 1"
        )?,
        cfg.nodes as u64
    );
    assert_eq!(
        sqlite_count(&conn, "SELECT COUNT(*) FROM edges WHERE rel = 1")?,
        cfg.edge_count() as u64
    );
    let reopen_verify_ms = elapsed_ms(reopen_start);

    let lookup = bench_sqlite_lookup_materialized(&conn, cfg)?;
    let hot = bench_sqlite_one_hop_hot_materialized(&conn, cfg)?;
    let cold = bench_sqlite_one_hop_cold_materialized(&conn, cfg)?;
    let incoming = bench_sqlite_incoming_cold_materialized(&conn, cfg)?;
    let two_hop = bench_sqlite_two_hop_materialized(&conn, cfg)?;
    let update = bench_sqlite_updates_materialized(&conn, cfg)?;
    let detach_delete = bench_sqlite_deletes_materialized(&conn, cfg)?;
    let correctness_hash = sqlite_materialized_correctness_hash(&conn, cfg)?;

    drop(conn);
    let (db_bytes, db_file_count) = path_stats(temp.path())?;

    Ok(RunMetrics {
        system_version: sqlite_version(&path)?,
        storage_path: path,
        load_nodes_ms,
        load_edges_ms,
        commit_ms,
        reopen_verify_ms,
        lookup: lookup.0,
        lookup_rows_total: lookup.1,
        one_hop_hot_edges_per_sec: hot,
        one_hop_cold_edges_per_sec: cold,
        incoming_cold_edges_per_sec: incoming,
        two_hop_paths_per_sec: two_hop,
        update,
        detach_delete,
        db_bytes,
        db_file_count,
        correctness_hash,
        notes: vec!["SQLite materialized graph keyspaces and property index".to_string()],
    })
}

fn create_sqlite_materialized_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE nodes (
          id INTEGER PRIMARY KEY,
          flags INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE labels (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE
        );
        CREATE TABLE reltypes (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE
        );
        CREATE TABLE node_labels (
          node_id INTEGER NOT NULL,
          label_id INTEGER NOT NULL,
          PRIMARY KEY (node_id, label_id)
        ) WITHOUT ROWID;
        CREATE INDEX node_labels_by_label_idx ON node_labels(label_id, node_id);
        CREATE TABLE node_props (
          node_id INTEGER NOT NULL,
          key TEXT NOT NULL,
          value_key TEXT NOT NULL,
          PRIMARY KEY (node_id, key)
        ) WITHOUT ROWID;
        CREATE TABLE idx_node_prop (
          label_id INTEGER NOT NULL,
          key TEXT NOT NULL,
          value_key TEXT NOT NULL,
          node_id INTEGER NOT NULL,
          PRIMARY KEY (label_id, key, value_key, node_id)
        ) WITHOUT ROWID;
        CREATE TABLE edges (
          src INTEGER NOT NULL,
          rel INTEGER NOT NULL,
          dst INTEGER NOT NULL,
          PRIMARY KEY (src, rel, dst)
        ) WITHOUT ROWID;
        CREATE INDEX edges_in_idx ON edges(dst, rel, src);
        INSERT INTO labels(id, name) VALUES (1, 'BenchNode');
        INSERT INTO reltypes(id, name) VALUES (1, 'LINK');
        ",
    )
}

fn load_sqlite_materialized(conn: &mut Connection, cfg: &Config) -> AnyResult<(f64, f64, f64)> {
    let tx = conn.transaction()?;

    let nodes_start = Instant::now();
    {
        let mut node_stmt = tx.prepare("INSERT INTO nodes(id, flags) VALUES (?, 0)")?;
        let mut label_stmt =
            tx.prepare("INSERT INTO node_labels(node_id, label_id) VALUES (?, 1)")?;
        let mut prop_stmt =
            tx.prepare("INSERT INTO node_props(node_id, key, value_key) VALUES (?, ?, ?)")?;
        let mut index_stmt = tx.prepare(
            "INSERT INTO idx_node_prop(label_id, key, value_key, node_id) VALUES (1, ?, ?, ?)",
        )?;

        for i in 0..cfg.nodes {
            let id = (i + 1) as i64;
            node_stmt.execute(params![id])?;
            label_stmt.execute(params![id])?;
            for (key, value_key) in node_properties(i) {
                prop_stmt.execute(params![id, key, value_key])?;
                index_stmt.execute(params![key, value_key, id])?;
            }
        }
    }
    let load_nodes_ms = elapsed_ms(nodes_start);

    let edges_start = Instant::now();
    {
        let mut stmt = tx.prepare("INSERT INTO edges(src, rel, dst) VALUES (?, 1, ?)")?;
        for (src_idx, dst_idx) in generated_edges(cfg) {
            stmt.execute(params![(src_idx + 1) as i64, (dst_idx + 1) as i64])?;
        }
    }
    let load_edges_ms = elapsed_ms(edges_start);

    let commit_start = Instant::now();
    tx.commit()?;
    let commit_ms = elapsed_ms(commit_start);
    Ok((load_nodes_ms, load_edges_ms, commit_ms))
}

fn bench_sqlite_lookup_materialized(
    conn: &Connection,
    cfg: &Config,
) -> AnyResult<(LatencySummary, u64)> {
    let mut stmt = conn.prepare(
        "SELECT node_id
         FROM idx_node_prop
         WHERE label_id = 1 AND key = 'name' AND value_key = ?
         LIMIT 1",
    )?;
    let target = value_string(&cfg.lookup_target_name());
    let mut samples = Vec::with_capacity(cfg.iters);
    let mut rows_total = 0;
    for _ in 0..cfg.iters {
        let start = Instant::now();
        let row: Option<i64> = stmt
            .query_row(params![target], |row| row.get(0))
            .optional()?;
        rows_total += u64::from(row.is_some());
        samples.push(elapsed_us(start));
    }
    Ok((LatencySummary::from_samples(samples), rows_total))
}

fn bench_sqlite_one_hop_hot_materialized(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare("SELECT dst FROM edges WHERE src = ? AND rel = 1")?;
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..cfg.iters {
        edges_total += stmt.query_map(params![1_i64], |_| Ok(()))?.count() as u64;
    }
    Ok(edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_one_hop_cold_materialized(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare("SELECT dst FROM edges WHERE src = ? AND rel = 1")?;
    let mut rng = SplitMix64::new(cfg.seed ^ 0x1111_2222_3333_4444);
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..cfg.iters {
        let src = random_node_id(&mut rng, cfg);
        edges_total += stmt.query_map(params![src], |_| Ok(()))?.count() as u64;
    }
    Ok(edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_incoming_cold_materialized(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare("SELECT src FROM edges WHERE dst = ? AND rel = 1")?;
    let mut rng = SplitMix64::new(cfg.seed ^ 0x5555_6666_7777_8888);
    let start = Instant::now();
    let mut edges_total = 0;
    for _ in 0..cfg.iters {
        let dst = random_node_id(&mut rng, cfg);
        edges_total += stmt.query_map(params![dst], |_| Ok(()))?.count() as u64;
    }
    Ok(edges_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_two_hop_materialized(conn: &Connection, cfg: &Config) -> AnyResult<f64> {
    let mut stmt = conn.prepare(
        "SELECT e2.dst
         FROM edges e1
         JOIN edges e2 ON e2.src = e1.dst AND e2.rel = 1
         WHERE e1.src = ? AND e1.rel = 1",
    )?;
    let mut rng = SplitMix64::new(cfg.seed ^ 0x9999_aaaa_bbbb_cccc);
    let start = Instant::now();
    let mut paths_total = 0;
    for _ in 0..cfg.iters {
        let src = random_node_id(&mut rng, cfg);
        paths_total += stmt.query_map(params![src], |_| Ok(()))?.count() as u64;
    }
    Ok(paths_total as f64 / start.elapsed().as_secs_f64().max(1e-9))
}

fn bench_sqlite_updates_materialized(conn: &Connection, cfg: &Config) -> AnyResult<LatencySummary> {
    let mut samples = Vec::with_capacity(cfg.mutation_count());
    for id in cfg.update_ids() {
        let start = Instant::now();
        conn.execute("BEGIN IMMEDIATE", [])?;
        conn.execute(
            "UPDATE node_props SET value_key = ? WHERE node_id = ? AND key = 'status'",
            params![value_string("updated"), id as i64],
        )?;
        conn.execute(
            "DELETE FROM idx_node_prop WHERE label_id = 1 AND key = 'status' AND node_id = ?",
            params![id as i64],
        )?;
        conn.execute(
            "INSERT INTO idx_node_prop(label_id, key, value_key, node_id) VALUES (1, 'status', ?, ?)",
            params![value_string("updated"), id as i64],
        )?;
        conn.execute("COMMIT", [])?;
        samples.push(elapsed_us(start));
    }
    Ok(LatencySummary::from_samples(samples))
}

fn bench_sqlite_deletes_materialized(conn: &Connection, cfg: &Config) -> AnyResult<LatencySummary> {
    let mut samples = Vec::with_capacity(cfg.mutation_count());
    for id in cfg.delete_ids() {
        let start = Instant::now();
        conn.execute("BEGIN IMMEDIATE", [])?;
        conn.execute(
            "DELETE FROM idx_node_prop WHERE node_id = ?",
            params![id as i64],
        )?;
        conn.execute(
            "DELETE FROM node_props WHERE node_id = ?",
            params![id as i64],
        )?;
        conn.execute(
            "DELETE FROM node_labels WHERE node_id = ?",
            params![id as i64],
        )?;
        conn.execute(
            "DELETE FROM edges WHERE src = ? OR dst = ?",
            params![id as i64, id as i64],
        )?;
        conn.execute("DELETE FROM nodes WHERE id = ?", params![id as i64])?;
        conn.execute("COMMIT", [])?;
        samples.push(elapsed_us(start));
    }
    Ok(LatencySummary::from_samples(samples))
}

fn sqlite_materialized_correctness_hash(conn: &Connection, cfg: &Config) -> AnyResult<String> {
    let mut parts = Vec::new();
    parts.push(format!(
        "node_count={}",
        sqlite_count(conn, "SELECT COUNT(*) FROM node_labels WHERE label_id = 1")?
    ));
    parts.push(format!(
        "edge_count={}",
        sqlite_count(conn, "SELECT COUNT(*) FROM edges WHERE rel = 1")?
    ));
    let target = value_string(&cfg.lookup_target_name());
    let lookup = collect_i64(
        conn,
        "SELECT node_id
         FROM idx_node_prop
         WHERE label_id = 1 AND key = 'name' AND value_key = ?
         ORDER BY node_id",
        &[&target],
    )?;
    parts.push(format!("lookup={lookup:?}"));
    let updated_count = sqlite_count_value(
        conn,
        "SELECT COUNT(*)
         FROM idx_node_prop
         WHERE label_id = 1 AND key = 'status' AND value_key = ?",
        &value_string("updated"),
    )?;
    parts.push(format!("updated_count={updated_count}"));
    let mid = (cfg.nodes / 2 + 1) as i64;
    let out = collect_i64(
        conn,
        "SELECT dst FROM edges WHERE src = ? AND rel = 1 ORDER BY dst",
        &[&mid],
    )?;
    parts.push(format!("out_mid={out:?}"));
    let incoming = collect_i64(
        conn,
        "SELECT src FROM edges WHERE dst = ? AND rel = 1 ORDER BY src",
        &[&mid],
    )?;
    parts.push(format!("in_mid={incoming:?}"));
    parts.push(format!(
        "two_hop_mid={}",
        sqlite_count_param(
            conn,
            "SELECT COUNT(*)
             FROM edges e1
             JOIN edges e2 ON e2.src = e1.dst AND e2.rel = 1
             WHERE e1.src = ? AND e1.rel = 1",
            mid,
        )?
    ));
    for id in cfg.delete_ids() {
        let exists =
            sqlite_count_param(conn, "SELECT COUNT(*) FROM nodes WHERE id = ?", id as i64)?;
        parts.push(format!("deleted_{id}={}", exists == 0));
    }
    Ok(stable_hash(&parts))
}

fn sqlite_count(conn: &Connection, sql: &str) -> rusqlite::Result<u64> {
    conn.query_row(sql, [], |row| row.get::<_, i64>(0))
        .map(|v| v as u64)
}

fn sqlite_count_param(conn: &Connection, sql: &str, value: i64) -> rusqlite::Result<u64> {
    conn.query_row(sql, params![value], |row| row.get::<_, i64>(0))
        .map(|v| v as u64)
}

fn sqlite_count_value(conn: &Connection, sql: &str, value: &str) -> rusqlite::Result<u64> {
    conn.query_row(sql, params![value], |row| row.get::<_, i64>(0))
        .map(|v| v as u64)
}

fn collect_i64(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> AnyResult<Vec<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows: Vec<i64> = stmt
        .query_map(params, |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.sort_unstable();
    Ok(rows)
}

fn sqlite_version(path: &Path) -> AnyResult<String> {
    let conn = Connection::open(path)?;
    let version: String = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    Ok(version)
}

fn generated_edges(cfg: &Config) -> impl Iterator<Item = (usize, usize)> + '_ {
    (0..cfg.nodes).flat_map(move |src_idx| {
        (0..cfg.degree).map(move |j| {
            let dst_idx = (src_idx + j + 1) % cfg.nodes;
            (src_idx, dst_idx)
        })
    })
}

fn node_name(i: usize) -> String {
    format!("node_{i}")
}

fn node_kind(i: usize) -> String {
    format!("kind_{}", i % 8)
}

fn value_string(value: &str) -> String {
    format!("s:{value}")
}

fn value_int(value: i64) -> String {
    format!("i:{value}")
}

fn node_properties(i: usize) -> [(&'static str, String); 4] {
    [
        ("name", value_string(&node_name(i))),
        ("kind", value_string(&node_kind(i))),
        ("status", value_string("active")),
        ("chapter", value_int((i % 64) as i64)),
    ]
}

fn random_node_id(rng: &mut SplitMix64, cfg: &Config) -> i64 {
    ((rng.next_u64() as usize % cfg.nodes) + 1) as i64
}

fn required_value(value: Option<String>, arg: &str) -> String {
    value.unwrap_or_else(|| {
        eprintln!("missing value for {arg}");
        std::process::exit(2);
    })
}

fn parse_usize(value: Option<String>, arg: &str) -> usize {
    required_value(value, arg).parse().unwrap_or_else(|_| {
        eprintln!("invalid integer for {arg}");
        std::process::exit(2);
    })
}

fn parse_u64(value: Option<String>, arg: &str) -> u64 {
    required_value(value, arg).parse().unwrap_or_else(|_| {
        eprintln!("invalid integer for {arg}");
        std::process::exit(2);
    })
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn elapsed_us(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000_000.0
}

fn percentile_us(mut samples: Vec<f64>, q: f64) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((samples.len() - 1) as f64 * q).round() as usize;
    samples[idx]
}

fn round3(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

fn stable_hash(parts: &[String]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in part.as_bytes().iter().chain(std::iter::once(&0xff)) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

fn path_stats(path: &Path) -> AnyResult<(u64, u64)> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let meta = fs::metadata(path)?;
    if meta.is_file() {
        return Ok((meta.len(), 1));
    }

    let mut bytes = 0;
    let mut files = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                bytes += meta.len();
                files += 1;
            }
        }
    }
    Ok((bytes, files))
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

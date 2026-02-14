//! v2 crash consistency verification tool ("crash gate").
//!
//! Model:
//! - `driver` spawns a `writer`, randomly SIGKILLs it, then runs `verify`.
//! - `writer` loops: begin tx -> (create nodes + edges) -> commit, occasionally compact.
//! - `verify` checks WAL/manifest structure and basic graph invariants.
//!
//! Usage:
//!   cargo run -p nervusdb-storage --bin nervusdb-crash-test -- driver <path> [--iterations N]
//!   cargo run -p nervusdb-storage --bin nervusdb-crash-test -- writer <path> [--batch N]
//!   cargo run -p nervusdb-storage --bin nervusdb-crash-test -- verify <path>
//!
//! Note: <path> is a base path; it will use <path>.ndb and <path>.wal.

use nervusdb_storage::csr::CsrSegment;
use nervusdb_storage::engine::GraphEngine;
use nervusdb_storage::pager::Pager;
use nervusdb_storage::wal::{SegmentPointer, Wal, WalRecord};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    #[cfg(target_arch = "wasm32")]
    {
        eprintln!("nervusdb-crash-test is not supported on wasm32 targets");
        return ExitCode::from(2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("[v2-crash-test] error: {err}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn real_main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    match mode.as_str() {
        "driver" => driver(parse_driver_args(args)?),
        "writer" => writer(parse_writer_args(args)?),
        "verify" => verify(parse_verify_args(args)?),
        _ => {
            print_usage();
            Err("invalid subcommand".to_string())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn print_usage() {
    eprintln!(
        "Usage:\n  nervusdb-crash-test driver <path> [--iterations N] [--min-ms A] [--max-ms B] [--batch N] [--node-pool N] [--rel-pool N]\n  nervusdb-crash-test writer <path> [--batch N] [--node-pool N] [--rel-pool N] [--compact-every N] [--seed S]\n  nervusdb-crash-test verify <path> [--node-pool N]\n\nNote: <path> is a base path; it will use <path>.ndb and <path>.wal."
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct DriverArgs {
    path: PathBuf,
    iterations: usize,
    min_delay_ms: u64,
    max_delay_ms: u64,
    batch_size: usize,
    node_pool: u64,
    rel_pool: u32,
    verify_retries: usize,
    verify_backoff_ms: u64,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct WriterArgs {
    path: PathBuf,
    batch_size: usize,
    node_pool: u64,
    rel_pool: u32,
    compact_every: usize,
    seed: u64,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct VerifyArgs {
    path: PathBuf,
    node_pool: u64,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_driver_args(mut args: impl Iterator<Item = String>) -> Result<DriverArgs, String> {
    let path = PathBuf::from(args.next().unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err("missing <path>".to_string());
    }

    let mut out = DriverArgs {
        path,
        iterations: 100,
        min_delay_ms: 2,
        max_delay_ms: 20,
        batch_size: 200,
        node_pool: 200,
        rel_pool: 16,
        verify_retries: 30,
        verify_backoff_ms: 20,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => out.iterations = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--min-ms" => out.min_delay_ms = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--max-ms" => out.max_delay_ms = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--batch" => out.batch_size = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--node-pool" => out.node_pool = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--rel-pool" => out.rel_pool = parse_u32(&next_value(&mut args, &arg)?, &arg)?,
            "--verify-retries" => {
                out.verify_retries = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--verify-backoff-ms" => {
                out.verify_backoff_ms = parse_u64(&next_value(&mut args, &arg)?, &arg)?;
            }
            _ => {
                print_usage();
                return Err(format!("unknown flag: {arg}"));
            }
        }
    }

    if out.min_delay_ms > out.max_delay_ms {
        return Err("--min-ms must be <= --max-ms".to_string());
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_writer_args(mut args: impl Iterator<Item = String>) -> Result<WriterArgs, String> {
    let path = PathBuf::from(args.next().unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err("missing <path>".to_string());
    }

    let mut out = WriterArgs {
        path,
        batch_size: 200,
        node_pool: 200,
        rel_pool: 16,
        compact_every: 10,
        seed: default_seed(),
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batch" => out.batch_size = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--node-pool" => out.node_pool = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--rel-pool" => out.rel_pool = parse_u32(&next_value(&mut args, &arg)?, &arg)?,
            "--compact-every" => {
                out.compact_every = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--seed" => out.seed = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            _ => {
                print_usage();
                return Err(format!("unknown flag: {arg}"));
            }
        }
    }

    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_verify_args(mut args: impl Iterator<Item = String>) -> Result<VerifyArgs, String> {
    let path = PathBuf::from(args.next().unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err("missing <path>".to_string());
    }

    let mut out = VerifyArgs {
        path,
        node_pool: 200,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--node-pool" => out.node_pool = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            _ => {
                print_usage();
                return Err(format!("unknown flag: {arg}"));
            }
        }
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_u64(raw: &str, name: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("invalid {name}: {raw}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_u32(raw: &str, name: &str) -> Result<u32, String> {
    raw.parse::<u32>()
        .map_err(|_| format!("invalid {name}: {raw}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_usize(raw: &str, name: &str) -> Result<usize, String> {
    raw.parse::<usize>()
        .map_err(|_| format!("invalid {name}: {raw}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn default_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u64;
    (nanos as u64) ^ pid.rotate_left(17)
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct XorShift64 {
    state: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn gen_range(&mut self, upper: u64) -> u64 {
        if upper <= 1 {
            return 0;
        }
        self.next_u64() % upper
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn derive_paths(base: &Path) -> (PathBuf, PathBuf) {
    match base.extension().and_then(|e| e.to_str()) {
        Some("ndb") => (base.to_path_buf(), base.with_extension("wal")),
        Some("wal") => (base.with_extension("ndb"), base.to_path_buf()),
        _ => (base.with_extension("ndb"), base.with_extension("wal")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn driver(args: DriverArgs) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut rng = XorShift64::new(default_seed());

    // Bootstrap: ensure we have at least one committed state before killing writers.
    bootstrap(&args.path, args.node_pool, args.rel_pool)?;

    for i in 0..args.iterations {
        let delay_ms = args.min_delay_ms + rng.gen_range(args.max_delay_ms - args.min_delay_ms + 1);

        let mut child = Command::new(&exe)
            .arg("writer")
            .arg(&args.path)
            .arg("--batch")
            .arg(args.batch_size.to_string())
            .arg("--node-pool")
            .arg(args.node_pool.to_string())
            .arg("--rel-pool")
            .arg(args.rel_pool.to_string())
            .arg("--compact-every")
            .arg("10")
            .arg("--seed")
            .arg(default_seed().to_string())
            .spawn()
            .map_err(|e| e.to_string())?;

        thread::sleep(Duration::from_millis(delay_ms));

        // SIGKILL
        let _ = child.kill();
        let _ = child.wait();

        // verify (with retries)
        let mut ok = false;
        for attempt in 0..=args.verify_retries {
            match verify(VerifyArgs {
                path: args.path.clone(),
                node_pool: args.node_pool,
            }) {
                Ok(()) => {
                    ok = true;
                    break;
                }
                Err(e) => {
                    if attempt == args.verify_retries {
                        return Err(format!("verify failed after retries: {e}"));
                    }
                    thread::sleep(Duration::from_millis(args.verify_backoff_ms));
                }
            }
        }

        if !ok {
            return Err("verify failed".to_string());
        }

        if i % 10 == 0 {
            eprintln!("[v2-crash-test] iterations: {}/{}", i + 1, args.iterations);
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn bootstrap(base: &Path, node_pool: u64, rel_pool: u32) -> Result<(), String> {
    let (ndb, wal) = derive_paths(base);
    let engine = GraphEngine::open(&ndb, &wal).map_err(|e| e.to_string())?;

    let mut tx = engine.begin_write();
    let a = match tx.create_node(1, 1) {
        Ok(iid) => iid,
        Err(_) => engine
            .lookup_internal_id(1)
            .ok_or_else(|| "bootstrap: node 1 missing".to_string())?,
    };
    let b = match tx.create_node(2, 1) {
        Ok(iid) => iid,
        Err(_) => engine
            .lookup_internal_id(2)
            .ok_or_else(|| "bootstrap: node 2 missing".to_string())?,
    };

    let rel = rel_pool.max(1);
    tx.create_edge(a, rel, b);
    let _ = tx.commit();

    // Try a compaction as well (exercise manifest path).
    let _ = engine.compact();

    // Ensure we can open cleanly.
    verify(VerifyArgs {
        path: base.to_path_buf(),
        node_pool,
    })?;

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn writer(args: WriterArgs) -> Result<(), String> {
    let (ndb, wal) = derive_paths(&args.path);
    let engine = GraphEngine::open(&ndb, &wal).map_err(|e| e.to_string())?;

    let mut rng = XorShift64::new(args.seed);
    let mut tx_counter: usize = 0;

    loop {
        let mut tx = engine.begin_write();

        // Create a few nodes opportunistically.
        for _ in 0..4 {
            let external_id = 1 + rng.gen_range(args.node_pool);
            let _ = tx.create_node(external_id, 1);
        }

        // Insert edges among existing nodes.
        for _ in 0..args.batch_size {
            let src_e = 1 + rng.gen_range(args.node_pool);
            let dst_e = 1 + rng.gen_range(args.node_pool);
            let Some(src) = engine.lookup_internal_id(src_e) else {
                continue;
            };
            let Some(dst) = engine.lookup_internal_id(dst_e) else {
                continue;
            };
            let rel = if args.rel_pool <= 1 {
                1
            } else {
                (rng.gen_range(args.rel_pool as u64) as u32) + 1
            };
            tx.create_edge(src, rel, dst);
        }

        let _ = tx.commit();

        tx_counter += 1;
        if args.compact_every > 0 && tx_counter % args.compact_every == 0 {
            let _ = engine.compact();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn verify(args: VerifyArgs) -> Result<(), String> {
    let (ndb, wal) = derive_paths(&args.path);

    // 1) WAL-level scan: manifest epochs should be monotonic; last manifest segments must load.
    let wal_reader = Wal::open(&wal).map_err(|e| e.to_string())?;
    let committed = wal_reader.replay_committed().map_err(|e| e.to_string())?;

    let (mut last_epoch, mut last_segments) = (0u64, Vec::<SegmentPointer>::new());
    for tx in &committed {
        for op in &tx.ops {
            if let WalRecord::ManifestSwitch {
                epoch, segments, ..
            } = op
            {
                if *epoch < last_epoch {
                    return Err("manifest epoch decreased".to_string());
                }
                last_epoch = *epoch;
                last_segments = segments.clone();
            }
        }
    }

    if !last_segments.is_empty() {
        let mut pager = Pager::open(&ndb).map_err(|e| e.to_string())?;
        for ptr in &last_segments {
            let seg = CsrSegment::load(&mut pager, ptr.meta_page_id).map_err(|e| e.to_string())?;
            if seg.id.0 != ptr.id {
                return Err("segment pointer id mismatch".to_string());
            }
        }
    }

    // 2) Graph-level scan: all visible edges must reference visible nodes (within pool).
    let engine = GraphEngine::open(&ndb, &wal).map_err(|e| e.to_string())?;

    let mut nodes: Vec<u32> = Vec::new();
    for external_id in 1..=args.node_pool {
        if let Some(iid) = engine.lookup_internal_id(external_id) {
            nodes.push(iid);
        }
    }

    let node_set: HashSet<u32> = nodes.iter().copied().collect();
    let snap = engine.begin_read();
    for src in nodes {
        for e in snap.neighbors(src, None) {
            if !node_set.contains(&e.dst) {
                return Err("edge points to unknown dst".to_string());
            }
        }
    }

    Ok(())
}

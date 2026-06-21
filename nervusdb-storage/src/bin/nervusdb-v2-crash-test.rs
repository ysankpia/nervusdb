//! Fjall-backed crash consistency smoke tool.
//!
//! Model:
//! - `driver` bootstraps a graph, repeatedly spawns `writer`, kills it, then
//!   runs `verify`.
//! - `writer` loops graph write transactions against a local database
//!   directory.
//! - `verify` reopens the directory and checks graph-level invariants.
//!
//! Usage:
//!   cargo run -p nervusdb-storage --bin nervusdb-v2-crash-test -- driver <dir>
//!   cargo run -p nervusdb-storage --bin nervusdb-v2-crash-test -- writer <dir>
//!   cargo run -p nervusdb-storage --bin nervusdb-v2-crash-test -- verify <dir>

use nervusdb::storage::engine::GraphEngine;
use nervusdb::{GraphSnapshot, PropertyValue};
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
            eprintln!("[fjall-crash-test] error: {err}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn real_main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    match args.next().unwrap_or_default().as_str() {
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
        "Usage:\n  nervusdb-v2-crash-test driver <dir> [--iterations N] [--min-ms A] [--max-ms B] [--batch N] [--node-pool N] [--rel-pool N]\n  nervusdb-v2-crash-test writer <dir> [--batch N] [--node-pool N] [--rel-pool N] [--persist-every N] [--seed S]\n  nervusdb-v2-crash-test verify <dir> [--node-pool N]"
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
    persist_every: usize,
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
    let path = parse_path(args.next())?;
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
    out.node_pool = out.node_pool.max(2);
    out.rel_pool = out.rel_pool.max(1);
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_writer_args(mut args: impl Iterator<Item = String>) -> Result<WriterArgs, String> {
    let path = parse_path(args.next())?;
    let mut out = WriterArgs {
        path,
        batch_size: 200,
        node_pool: 200,
        rel_pool: 16,
        persist_every: 10,
        seed: default_seed(),
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batch" => out.batch_size = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--node-pool" => out.node_pool = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--rel-pool" => out.rel_pool = parse_u32(&next_value(&mut args, &arg)?, &arg)?,
            "--persist-every" | "--compact-every" => {
                out.persist_every = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--seed" => out.seed = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            _ => {
                print_usage();
                return Err(format!("unknown flag: {arg}"));
            }
        }
    }

    out.node_pool = out.node_pool.max(2);
    out.rel_pool = out.rel_pool.max(1);
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_verify_args(mut args: impl Iterator<Item = String>) -> Result<VerifyArgs, String> {
    let path = parse_path(args.next())?;
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
    out.node_pool = out.node_pool.max(2);
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_path(path: Option<String>) -> Result<PathBuf, String> {
    let path = PathBuf::from(path.unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err("missing <dir>".to_string());
    }
    Ok(path)
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
    (nanos as u64) ^ (std::process::id() as u64).rotate_left(17)
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
            0
        } else {
            self.next_u64() % upper
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn driver(args: DriverArgs) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut rng = XorShift64::new(default_seed());

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
            .arg("--persist-every")
            .arg("10")
            .arg("--seed")
            .arg(default_seed().to_string())
            .spawn()
            .map_err(|e| e.to_string())?;

        thread::sleep(Duration::from_millis(delay_ms));
        let _ = child.kill();
        let _ = child.wait();

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
                Err(err) if attempt < args.verify_retries => {
                    eprintln!("[fjall-crash-test] verify retry after: {err}");
                    thread::sleep(Duration::from_millis(args.verify_backoff_ms));
                }
                Err(err) => return Err(format!("verify failed after retries: {err}")),
            }
        }

        if !ok {
            return Err("verify failed".to_string());
        }
        if i % 10 == 0 {
            eprintln!(
                "[fjall-crash-test] iterations: {}/{}",
                i + 1,
                args.iterations
            );
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn bootstrap(path: &Path, node_pool: u64, _rel_pool: u32) -> Result<(), String> {
    let engine = GraphEngine::open(path).map_err(|e| e.to_string())?;
    let mut tx = engine.begin_write();
    let label = tx
        .get_or_create_label("CrashNode")
        .map_err(|e| e.to_string())?;
    let rel = tx
        .get_or_create_rel_type("CRASH_REL_0")
        .map_err(|e| e.to_string())?;

    let a = ensure_node(&engine, &mut tx, 1, label)?;
    let b = ensure_node(&engine, &mut tx, node_pool.max(2), label)?;
    tx.create_edge(a, rel, b);
    tx.set_node_property(a, "seed".to_string(), "bootstrap".into());
    tx.commit().map_err(|e| e.to_string())?;
    engine.persist().map_err(|e| e.to_string())?;
    drop(engine);

    verify(VerifyArgs {
        path: path.to_path_buf(),
        node_pool,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn writer(args: WriterArgs) -> Result<(), String> {
    let engine = GraphEngine::open(&args.path).map_err(|e| e.to_string())?;
    let mut rng = XorShift64::new(args.seed);
    let mut tx_counter: usize = 0;

    loop {
        let mut tx = engine.begin_write();
        let label = tx
            .get_or_create_label("CrashNode")
            .map_err(|e| e.to_string())?;

        for _ in 0..4 {
            let external_id = 1 + rng.gen_range(args.node_pool);
            if let Ok(iid) = tx.create_node(external_id, label) {
                tx.set_node_property(iid, "kind".to_string(), "crash".into());
            }
        }

        for _ in 0..args.batch_size {
            let src_e = 1 + rng.gen_range(args.node_pool);
            let dst_e = 1 + rng.gen_range(args.node_pool);
            let Some(src) = engine.lookup_internal_id(src_e) else {
                continue;
            };
            let Some(dst) = engine.lookup_internal_id(dst_e) else {
                continue;
            };
            let rel_idx = rng.gen_range(args.rel_pool as u64) as u32;
            let rel = tx
                .get_or_create_rel_type(&format!("CRASH_REL_{rel_idx}"))
                .map_err(|e| e.to_string())?;
            tx.create_edge(src, rel, dst);
            tx.set_edge_property(src, rel, dst, "written".to_string(), true.into());
        }

        let _ = tx.commit();
        tx_counter += 1;
        if args.persist_every > 0 && tx_counter % args.persist_every == 0 {
            let _ = engine.persist();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_node(
    engine: &GraphEngine,
    tx: &mut nervusdb::storage::engine::WriteTxn<'_>,
    external_id: u64,
    label: u32,
) -> Result<u32, String> {
    match tx.create_node(external_id, label) {
        Ok(iid) => Ok(iid),
        Err(_) => engine
            .lookup_internal_id(external_id)
            .ok_or_else(|| format!("node {external_id} missing")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn verify(args: VerifyArgs) -> Result<(), String> {
    let engine = GraphEngine::open(&args.path).map_err(|e| e.to_string())?;
    let snap = engine.begin_read();
    let nodes: Vec<u32> = snap.nodes().collect();
    let node_set: HashSet<u32> = nodes.iter().copied().collect();

    if nodes.is_empty() {
        return Err("no visible nodes after reopen".to_string());
    }

    let label = snap
        .resolve_label_id("CrashNode")
        .ok_or_else(|| "CrashNode label missing".to_string())?;
    let labelled: HashSet<u32> = snap.nodes_with_label(label).collect();
    if labelled.is_empty() {
        return Err("label scan returned no CrashNode nodes".to_string());
    }
    if !labelled.is_subset(&node_set) {
        return Err("label scan returned unknown node".to_string());
    }

    for src in &nodes {
        for edge in snap.neighbors(*src, None) {
            if !node_set.contains(&edge.src) || !node_set.contains(&edge.dst) {
                return Err("edge endpoint is not a visible node".to_string());
            }
            if let Some(value) = snap.edge_property(edge, "written") {
                let expected = PropertyValue::Bool(true);
                if value != expected {
                    return Err("edge property decode mismatch".to_string());
                }
            }
        }
    }

    for external_id in 1..=args.node_pool {
        if let Some(iid) = engine.lookup_internal_id(external_id)
            && !node_set.contains(&iid)
        {
            return Err("external id resolved to invisible node".to_string());
        }
    }

    Ok(())
}

//! Crash consistency verification tool ("fuck-off test").
//!
//! This is intentionally simple:
//! - `driver` spawns a `writer`, randomly SIGKILLs it, then runs `verify`.
//! - `writer` loops: begin tx -> insert batch -> commit.
//! - `verify` checks dictionary roundtrips and triple references.
//!
//! Usage:
//!   cargo run -p nervusdb-core --bin nervus-crash-test -- driver <path> [--iterations N]
//!   cargo run -p nervusdb-core --bin nervus-crash-test -- verify <path>
//!   cargo run -p nervusdb-core --bin nervus-crash-test -- writer <path>

use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nervusdb_core::{Database, Fact, Options, QueryCriteria, Result, StringId};

fn main() -> ExitCode {
    #[cfg(target_arch = "wasm32")]
    {
        eprintln!("nervus-crash-test is not supported on wasm32 targets");
        return ExitCode::from(2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("[crash-test] error: {err}");
            ExitCode::from(1)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn real_main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_default();
    match mode.as_str() {
        "driver" => driver(parse_driver_args(args)?),
        "writer" => writer(parse_writer_args(args)?),
        "verify" => verify(parse_verify_args(args)?),
        _ => {
            print_usage();
            Err(nervusdb_core::Error::Other(
                "invalid subcommand".to_string(),
            ))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn print_usage() {
    eprintln!(
        "Usage:\n  nervus-crash-test driver <path> [--iterations N] [--min-ms A] [--max-ms B] [--batch N]\n  nervus-crash-test writer <path> [--batch N]\n  nervus-crash-test verify <path>\n\nNote: <path> is the same base path you pass to Database::open (it will use <path>.redb)."
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
    subject_pool: usize,
    predicate_pool: usize,
    object_pool: usize,
    verify_retries: usize,
    verify_backoff_ms: u64,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct WriterArgs {
    path: PathBuf,
    batch_size: usize,
    subject_pool: usize,
    predicate_pool: usize,
    object_pool: usize,
    seed: u64,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct VerifyArgs {
    path: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_driver_args(mut args: impl Iterator<Item = String>) -> Result<DriverArgs> {
    let path = PathBuf::from(args.next().unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err(nervusdb_core::Error::Other("missing <path>".to_string()));
    }

    let mut out = DriverArgs {
        path,
        iterations: 200,
        min_delay_ms: 5,
        max_delay_ms: 50,
        batch_size: 5_000,
        subject_pool: 10_000,
        predicate_pool: 256,
        object_pool: 10_000,
        verify_retries: 50,
        verify_backoff_ms: 20,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iterations" => out.iterations = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--min-ms" => out.min_delay_ms = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--max-ms" => out.max_delay_ms = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--batch" => out.batch_size = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--subject-pool" => {
                out.subject_pool = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--predicate-pool" => {
                out.predicate_pool = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--object-pool" => out.object_pool = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--verify-retries" => {
                out.verify_retries = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--verify-backoff-ms" => {
                out.verify_backoff_ms = parse_u64(&next_value(&mut args, &arg)?, &arg)?;
            }
            _ => {
                print_usage();
                return Err(nervusdb_core::Error::Other(format!("unknown flag: {arg}")));
            }
        }
    }

    if out.min_delay_ms > out.max_delay_ms {
        return Err(nervusdb_core::Error::Other(
            "--min-ms must be <= --max-ms".to_string(),
        ));
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_writer_args(mut args: impl Iterator<Item = String>) -> Result<WriterArgs> {
    let path = PathBuf::from(args.next().unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err(nervusdb_core::Error::Other("missing <path>".to_string()));
    }

    let mut out = WriterArgs {
        path,
        batch_size: 5_000,
        subject_pool: 10_000,
        predicate_pool: 256,
        object_pool: 10_000,
        seed: default_seed(),
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--batch" => out.batch_size = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--subject-pool" => {
                out.subject_pool = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--predicate-pool" => {
                out.predicate_pool = parse_usize(&next_value(&mut args, &arg)?, &arg)?;
            }
            "--object-pool" => out.object_pool = parse_usize(&next_value(&mut args, &arg)?, &arg)?,
            "--seed" => out.seed = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            _ => {
                print_usage();
                return Err(nervusdb_core::Error::Other(format!("unknown flag: {arg}")));
            }
        }
    }

    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_verify_args(mut args: impl Iterator<Item = String>) -> Result<VerifyArgs> {
    let path = PathBuf::from(args.next().unwrap_or_default());
    if path.as_os_str().is_empty() {
        print_usage();
        return Err(nervusdb_core::Error::Other("missing <path>".to_string()));
    }
    Ok(VerifyArgs { path })
}

#[cfg(not(target_arch = "wasm32"))]
fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| nervusdb_core::Error::Other(format!("missing value for {flag}")))
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_u64(raw: &str, name: &str) -> Result<u64> {
    raw.parse::<u64>()
        .map_err(|_| nervusdb_core::Error::Other(format!("invalid {name}: {raw}")))
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_usize(raw: &str, name: &str) -> Result<usize> {
    raw.parse::<usize>()
        .map_err(|_| nervusdb_core::Error::Other(format!("invalid {name}: {raw}")))
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
fn driver(args: DriverArgs) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| nervusdb_core::Error::Other(e.to_string()))?;
    let mut rng = XorShift64::new(default_seed());

    for i in 0..args.iterations {
        let mut child = Command::new(&exe)
            .arg("writer")
            .arg(&args.path)
            .arg("--batch")
            .arg(args.batch_size.to_string())
            .arg("--subject-pool")
            .arg(args.subject_pool.to_string())
            .arg("--predicate-pool")
            .arg(args.predicate_pool.to_string())
            .arg("--object-pool")
            .arg(args.object_pool.to_string())
            .spawn()
            .map_err(|e| nervusdb_core::Error::Other(e.to_string()))?;

        let delay_ms = args.min_delay_ms + rng.gen_range(args.max_delay_ms - args.min_delay_ms + 1);
        thread::sleep(Duration::from_millis(delay_ms));

        // SIGKILL
        let _ = child.kill();
        let _ = child.wait();

        verify_with_retries(&args.path, args.verify_retries, args.verify_backoff_ms).map_err(
            |e| {
                nervusdb_core::Error::Other(format!(
                    "verify failed after crash (iter={i}, delay_ms={delay_ms}): {e}"
                ))
            },
        )?;
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn writer(args: WriterArgs) -> Result<()> {
    let mut rng = XorShift64::new(args.seed);
    let subjects = build_pool("s", args.subject_pool);
    let predicates = build_pool("p", args.predicate_pool);
    let objects = build_pool("o", args.object_pool);

    let mut db = Database::open(Options::new(&args.path))?;
    loop {
        db.begin_transaction()?;
        insert_batch(
            &mut db,
            &subjects,
            &predicates,
            &objects,
            args.batch_size,
            &mut rng,
        )?;
        db.commit_transaction()?;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_pool(prefix: &str, size: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(size.max(1));
    for i in 0..size.max(1) {
        out.push(format!("{prefix}{i}"));
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn insert_batch(
    db: &mut Database,
    subjects: &[String],
    predicates: &[String],
    objects: &[String],
    batch_size: usize,
    rng: &mut XorShift64,
) -> Result<()> {
    let s_upper = subjects.len() as u64;
    let p_upper = predicates.len() as u64;
    let o_upper = objects.len() as u64;

    for _ in 0..batch_size.max(1) {
        let s = &subjects[rng.gen_range(s_upper) as usize];
        let p = &predicates[rng.gen_range(p_upper) as usize];
        let o = &objects[rng.gen_range(o_upper) as usize];
        db.add_fact(Fact::new(s, p, o))?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn verify(args: VerifyArgs) -> Result<()> {
    verify_db(&args.path)
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_with_retries(path: &PathBuf, retries: usize, backoff_ms: u64) -> Result<()> {
    let attempts = retries.max(1);
    let backoff = Duration::from_millis(backoff_ms.max(1));

    for attempt in 1..=attempts {
        match verify_db(path) {
            Ok(()) => return Ok(()),
            Err(err) if attempt < attempts => {
                eprintln!("[crash-test] verify attempt {attempt}/{attempts} failed: {err}");
                thread::sleep(backoff);
            }
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_db(path: &PathBuf) -> Result<()> {
    let db = Database::open(Options::new(path))?;
    verify_dictionary(&db)?;
    verify_triples(&db)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_dictionary(db: &Database) -> Result<()> {
    let size = db.dictionary_size()?;
    let has_zero = db.resolve_str(0)?.is_some();
    let start: StringId = if has_zero { 0 } else { 1 };
    let end: StringId = if has_zero {
        size.saturating_sub(1)
    } else {
        size
    };

    for id in start..=end {
        let s = db
            .resolve_str(id)?
            .ok_or_else(|| dictionary_error("missing id_to_str", id))?;
        let back = db
            .resolve_id(&s)?
            .ok_or_else(|| dictionary_error("missing str_to_id", id))?;
        if back != id {
            return Err(nervusdb_core::Error::Other(format!(
                "dictionary mismatch: id={id} -> {s:?} -> id={back}"
            )));
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn dictionary_error(kind: &str, id: StringId) -> nervusdb_core::Error {
    nervusdb_core::Error::Other(format!("dictionary invalid ({kind}): id={id}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_triples(db: &Database) -> Result<()> {
    for triple in db.query(QueryCriteria::default()) {
        verify_triple_ids(db, triple.subject_id, triple.predicate_id, triple.object_id)?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_triple_ids(db: &Database, s: StringId, p: StringId, o: StringId) -> Result<()> {
    verify_id_roundtrip(db, s)?;
    verify_id_roundtrip(db, p)?;
    verify_id_roundtrip(db, o)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn verify_id_roundtrip(db: &Database, id: StringId) -> Result<()> {
    let s = db.resolve_str(id)?.ok_or_else(|| {
        nervusdb_core::Error::Other(format!("triple references missing id: {id}"))
    })?;
    let back = db.resolve_id(&s)?.ok_or_else(|| {
        nervusdb_core::Error::Other(format!("resolve_id failed for resolved string: {s:?}"))
    })?;
    if back != id {
        return Err(nervusdb_core::Error::Other(format!(
            "id roundtrip mismatch: id={id} -> {s:?} -> id={back}"
        )));
    }
    Ok(())
}

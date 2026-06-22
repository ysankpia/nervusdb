use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("NERVUSDB_PROFILE_STORAGE").is_some())
}

pub(crate) fn event(stage: &str, elapsed: Duration, fields: &[(&str, u64)]) {
    if !enabled() {
        return;
    }

    eprint!(
        "nervusdb_storage_profile stage={} elapsed_us={}",
        stage,
        elapsed.as_micros()
    );
    for (key, value) in fields {
        eprint!(" {key}={value}");
    }
    eprintln!();
}

pub(crate) fn start() -> Option<Instant> {
    enabled().then(Instant::now)
}

pub(crate) fn event_since(stage: &str, started: Option<Instant>, fields: &[(&str, u64)]) {
    if let Some(started) = started {
        event(stage, started.elapsed(), fields);
    }
}

pub(crate) fn edge_scan(
    stage: &'static str,
    started: Option<Instant>,
    scanned: u64,
    decoded: u64,
    live: u64,
) {
    let Some(started) = started else {
        return;
    };

    let aggregate = match stage {
        "neighbors" => &NEIGHBORS,
        "incoming_neighbors" => &INCOMING_NEIGHBORS,
        _ => {
            event(
                stage,
                started.elapsed(),
                &[("scanned", scanned), ("decoded", decoded), ("live", live)],
            );
            return;
        }
    };

    let calls = aggregate.calls.fetch_add(1, Ordering::Relaxed) + 1;
    aggregate.elapsed_us.fetch_add(
        u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    aggregate.scanned.fetch_add(scanned, Ordering::Relaxed);
    aggregate.decoded.fetch_add(decoded, Ordering::Relaxed);
    aggregate.live.fetch_add(live, Ordering::Relaxed);

    if calls <= 10 || calls % 10_000 == 0 {
        eprintln!(
            "nervusdb_storage_profile stage={} calls={} elapsed_us={} scanned={} decoded={} live={}",
            stage,
            calls,
            aggregate.elapsed_us.load(Ordering::Relaxed),
            aggregate.scanned.load(Ordering::Relaxed),
            aggregate.decoded.load(Ordering::Relaxed),
            aggregate.live.load(Ordering::Relaxed),
        );
    }
}

struct EdgeScanAggregate {
    calls: AtomicU64,
    elapsed_us: AtomicU64,
    scanned: AtomicU64,
    decoded: AtomicU64,
    live: AtomicU64,
}

impl EdgeScanAggregate {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            elapsed_us: AtomicU64::new(0),
            scanned: AtomicU64::new(0),
            decoded: AtomicU64::new(0),
            live: AtomicU64::new(0),
        }
    }
}

static NEIGHBORS: EdgeScanAggregate = EdgeScanAggregate::new();
static INCOMING_NEIGHBORS: EdgeScanAggregate = EdgeScanAggregate::new();

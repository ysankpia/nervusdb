use std::sync::OnceLock;
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

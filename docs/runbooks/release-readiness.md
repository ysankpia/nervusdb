# Release Readiness Runbook

This is for current 0.x / 0.1-core readiness only. It is not a revival of the old
platform release window.

Per ADR 0006, the public release artifact is one crate: `nervusdb`.

## Required Evidence

- `bash scripts/check.sh` passes.
- `bash scripts/core_smoke.sh` passes.
- `bash scripts/core_examples.sh` passes.
- Crash recovery evidence exists for the current storage model.
- `docs/reference/mini-cypher.md` matches the core acceptance tests.
- Ten realistic examples are documented in `docs/reference/examples-0.1.md`.
- Storage format and compatibility expectations are documented.
- Manual benchmark evidence exists for the chosen release candidate.
- Fsck / freeze smoke evidence exists when preparing `v0.0.5` or later.
- `cargo publish -p nervusdb --dry-run` passes without requiring users to depend
  on internal implementation crates.

## Not Required By Default

- Full openCypher TCK pass rate.
- Binding parity gates.
- Vector or HNSW benchmarks.
- Scheduled chaos, soak, fuzz, perf, or stability windows.

Those checks may be useful for targeted changes, but they are not release
blockers for the embedded graph 0.1 line unless a future ADR changes that rule.

## Publish Shape

Do not publish `nervusdb-api`, `nervusdb-storage`, or `nervusdb-query` as public
public crates. They are internal engineering boundaries unless a future ADR gives
one of them a real external audience.

Expected user install:

```toml
[dependencies]
nervusdb = "0.0.6"
```

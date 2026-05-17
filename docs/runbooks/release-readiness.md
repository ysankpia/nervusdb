# Release Readiness Runbook

This is for 0.1 readiness only. It is not a revival of the old platform release
window.

## Required Evidence

- `bash scripts/check.sh` passes.
- `bash scripts/core_smoke.sh` passes.
- Crash recovery evidence exists for the current storage model.
- `docs/reference/mini-cypher.md` matches the core acceptance tests.
- Ten realistic examples are documented or runnable.
- Storage format and compatibility expectations are documented.
- Manual benchmark evidence exists for the chosen release candidate.

## Not Required By Default

- Full openCypher TCK pass rate.
- Binding parity gates.
- Vector or HNSW benchmarks.
- Scheduled chaos, soak, fuzz, perf, or stability windows.

Those checks may be useful for targeted changes, but they are not release
blockers for the embedded graph 0.1 line unless a future ADR changes that rule.

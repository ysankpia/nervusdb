# Local Validation Runbook

## Normal 0.1 Loop

```bash
bash scripts/check.sh
```

This is the default validation gate. It is meant to be minutes, not an hour.

## Narrow Iteration

Use narrower commands while developing:

```bash
cargo fmt --all -- --check
cargo clippy -p nervusdb-query --lib -- -W warnings
bash scripts/workspace_quick_test.sh
```

For docs-only or CI-only edits, do not run full Rust tests by reflex. Use:

```bash
bash -n scripts/check.sh
bash -n scripts/workspace_quick_test.sh
rg "schedule:|cron:" .github/workflows
```

Then run the smallest targeted Rust test only if the changed files affect Rust
behavior or examples.

## Area-Specific Checks

- Query compatibility: `make tck-tier0`, `make tck-tier1`, `make tck-tier2`
- Bindings: `bash scripts/binding_smoke.sh`
- Cross-binding parity: `bash scripts/binding_parity_gate.sh`
- Performance: `bash scripts/perf_slo_gate.sh`
- Stability window evidence: `bash scripts/stability_window.sh`
- Core smoke: `bash scripts/core_smoke.sh`
- Core crash recovery: `bash scripts/core_crash_recovery.sh`
- Core benchmark: `bash scripts/core_bench.sh --small`
- Full historical workspace test: `bash scripts/workspace_full_test.sh`
- Full workspace clippy: included in `bash scripts/workspace_full_test.sh`

These checks are not part of the default 0.1 loop unless the PR touches the
corresponding area. Historical gates are manual-only. They do not run on a
schedule and do not block ordinary 0.1 changes.

## Large 0.1 Acceptance Runs

Large acceptance runs are not CI jobs. Record hardware, command, data scale, and
P50/P95/P99 output when running them:

```bash
bash scripts/core_bench.sh --large
```

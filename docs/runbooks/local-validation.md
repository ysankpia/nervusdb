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
cargo clippy -p nervusdb --lib -- -W warnings
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

- Core smoke: `bash scripts/core_smoke.sh`
- Core examples: `bash scripts/core_examples.sh`
- Core crash recovery: `bash scripts/core_crash_recovery.sh`
- Core benchmark: `bash scripts/core_bench.sh --small`
- Full workspace test: `cargo test --workspace`
- Broader clippy when needed: `cargo clippy --workspace --all-targets -- -W warnings`

These checks are not part of the default 0.1 loop unless the PR touches the
corresponding area. Platform-era binding/TCK/performance gates are archived
history, not current 0.1 requirements, unless a future ADR restores them.

Fjall storage changes should run at least:

```bash
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_rust_api
bash scripts/core_crash_recovery.sh
```

## Large 0.1 Acceptance Runs

Large acceptance runs are not CI jobs. Record hardware, command, data scale, and
P50/P95/P99 output when running them:

```bash
bash scripts/core_bench.sh --large
```

Large runs are release-candidate evidence. Do not add them to `check`, `quick`,
pre-commit, or pre-push paths.

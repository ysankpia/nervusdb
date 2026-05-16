# Local Validation Runbook

## Normal 0.1 Loop

```bash
bash scripts/check.sh
```

This is the default before opening a PR.

## Narrow Iteration

Use narrower commands while developing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
```

## Area-Specific Checks

- Query compatibility: `make tck-tier0`, `make tck-tier1`, `make tck-tier2`
- Bindings: `bash scripts/binding_smoke.sh`
- Cross-binding parity: `bash scripts/binding_parity_gate.sh`
- Performance: `bash scripts/perf_slo_gate.sh`
- Stability window evidence: `bash scripts/stability_window.sh`

These checks are not part of the default 0.1 loop unless the PR touches the
corresponding area.

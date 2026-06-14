# Technical Debt Ledger

## Active Debt

| Area | Debt | Impact | Plan |
|---|---|---|---|
| Scripts | 36 scripts in `scripts/`; many are historical and undocumented | Noise; unclear which scripts are default-loop vs manual-only | Categorize and document after 0.1 core stabilizes |
| CI workflows | 11 workflows in `.github/workflows/`; only `ci.yml` is default | Visual clutter; 10 nightly/manual workflows add maintenance surface | Clean up after 0.1; prioritize core CI reliability |
| Bindings in workspace | `nervusdb-pyo3`, `nervusdb-node`, `nervusdb-capi` are workspace members without feature isolation | Adds build surface; can cause workspace-wide test failures | Feature-gate or move out of workspace after 0.1 |
| HNSW/vector in storage | Vector index code lives inside `nervusdb-storage` with no feature gate | Adds complexity to the storage crate for non-core functionality | Feature-gate or extract after 0.1 |
| Binding test suites | `examples-test/` contains Python and Node.js capability tests | Maintenance burden; not part of default validation loop | Document as manual-only; revisit after 0.1 |
| File format epoch | `STORAGE_FORMAT_EPOCH = 1` defined but no focused fail-fast test for mismatch | Weak invariant enforcement | Add test during storage hardening |

## Deferred Cleanup

| Area | Description | Reason For Deferral |
|---|---|---|
| Script index | No single document lists all scripts with owner and purpose | Acceptable before 0.1; core scripts are documented in runbooks |
| Makefile TCK targets | `make tck-tier0` through `tck-tier3` are historical | Not default-loop; kept for manual compatibility checks |
| Legacy archive structure | Some archived docs may be rescuable or deletable | Not worth the audit time before 0.1 |
| Doc cross-reference audit | Some older docs reference each other without going through `docs/index.md` | Acceptable drift; will be caught in next gardening pass |

## Accepted Debt

| Area | Rationale |
|---|---|
| Experimental bindings remain workspace members | Removing them before 0.1 would break build CI for existing consumers; soft isolation via docs is sufficient |
| `scripts/` historical entries remain | Deleting them risks losing manual validation knowledge; document only |
| No Cargo feature isolation for experimental code | Features add Cargo resolver complexity; soft isolation via docs and clippy scope is sufficient before 0.1 |

## Retired Debt

| Area | Retired When | Reason |
|---|---|---|
| Platform-era legacy docs in main doc tree | 001-harness-normalization | Archived under `docs/archive/legacy-platform-era/` |
| Full-workspace test hidden behind quick name | 002-core-0.1-slimdown | `workspace_quick_test.sh` and `workspace_full_test.sh` are now distinct |
| Branching strategy undocumented | This backfill | Merged into `docs/engineering/git-workflow.md` |

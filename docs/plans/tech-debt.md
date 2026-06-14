# Technical Debt Ledger

## Active Debt

| Area | Debt | Impact | Plan |
|---|---|---|---|
| Dead code in storage | `backup.rs`, `bulkload.rs`, `vacuum.rs` (1,503 lines) still compiled in nervusdb-storage | Increases build time and binary size; API surface confusion | `git rm` these files and remove `pub mod` from lib.rs |
| Dead plan variants | Plan::MatchIn, MatchUndirected, IndexSeek, Apply, ProcedureCall, Foreach still exist in source | Return error at runtime; 200+ lines of never-constructed variants | Delete variants after verifying no parser path reaches them |
| Merge fields in PreparedQuery | merge_on_create_items, merge_on_match_items, etc. (6 fields) | Never read; wastes reader attention | Delete fields when touching query_api.rs next |
| No large-scale smoke | No documented 1M node / 5M edge acceptance test | Cannot prove scale readiness | Write and document a large smoke after core stabilizes |
| No benchmark baseline | `core_bench.sh` exists but no regression detection pipeline | Performance drift invisible | Add benchmark recording and comparison after 0.1 |
| No rustdoc on public API | Db, WriteTxn, ReadTxn have near-zero doc comments | User cannot learn API without reading source | Add rustdoc before 0.1 release |
| File format epoch | `STORAGE_FORMAT_EPOCH = 1` defined but no focused fail-fast test for mismatch | Weak invariant enforcement | Add test during storage hardening |
| No crates.io release | Version 0.0.1, never published | Nobody can install it | Publish after docs and smoke pass |

## Deferred Cleanup

| Area | Description | Reason For Deferral |
|---|---|---|
| Legacy archive structure | Some archived docs may be rescuable or deletable | Not worth the audit time before 0.1 |
| Doc cross-reference audit | Some older docs reference each other without going through `docs/index.md` | Acceptable drift; will be caught in next gardening pass |

## Accepted Debt

| Area | Rationale |
|---|---|
| backup/bulkload/vacuum still compiled | Deleting them is 30 min of work and blocked only by priority — should be done soon |
| Dead plan variants remain | Parsing still produces them for complex queries; verifying they're unreachable requires deeper analysis |
| No Cargo feature isolation | Features add Cargo resolver complexity; soft isolation via docs and clippy scope is sufficient before 0.1 |

## Retired Debt

| Area | Retired When | Reason |
|---|---|---|
| Platform-era legacy docs in main doc tree | 001-harness-normalization | Archived under `docs/archive/legacy-platform-era/` |
| Branching strategy undocumented | 007-harness-doc-backfill | Merged into `docs/engineering/git-workflow.md` |
| 36 historical scripts | 009-slim-to-0.1 | Deleted; 6 core scripts remain |
| 11 CI workflows | 009-slim-to-0.1 | Deleted; only ci.yml remains |
| Bindings (pyo3, capi, node) | 009-slim-to-0.1 | Deleted from workspace |
| HNSW/vector in storage | 009-slim-to-0.1 | Deleted from storage crate |
| Binding test suites | 009-slim-to-0.1 | examples-test/ deleted |
| Makefile TCK targets | 009-slim-to-0.1 | Makefile deleted |

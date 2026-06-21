# Technical Debt Ledger

## Active Debt

| Area | Debt | Impact | Plan |
|---|---|---|---|
| Property index ambiguity | `create_index/lookup_index` exists but is not 0.1 core | Scope drift and planner/storage ambiguity | Classify as experimental; no `prop_index` until future ADR |
| Query warning debt | `nervusdb-query` has unused imports, unused helpers, and MSRV-related clippy warnings | Noise in validation output; can hide real warnings later | Separate cleanup PR; do not mix with storage refactor |
| Experimental advanced query code | Parser/executor still contain features outside 0.1 core | Future tests can accidentally promote non-core behavior again | Keep out of core gates; document only when a future ADR promotes it |
| Tombstone secondary cleanup | Node tombstone hides nodes but does not eagerly remove every secondary key | Disk space and internal keyspace drift until future cleanup | Define delete/tombstone compaction after core API stabilizes |
| No large release-scale smoke | 10k node / 50k edge smoke passes, but no documented 1M node / 5M edge acceptance result | Cannot prove release-scale readiness | Run and record large smoke after Fjall core stabilizes |
| No benchmark baseline | `core_bench.sh` exists but no regression detection pipeline | Performance drift invisible | Add benchmark recording and comparison after 0.1 |

## Deferred Cleanup

| Area | Description | Reason For Deferral |
|---|---|---|
| Legacy archive structure | Some archived docs may be rescuable or deletable | Not worth the audit time before Fjall core lands |
| Doc cross-reference audit | Some older docs reference each other without going through `docs/index.md` | Acceptable drift; validate current path first |
| Old bd PB tasks | Existing beads still describe pager/page-cache work | ADR 0005 supersedes that direction; close or supersede after code lands |

## Accepted Debt

| Area | Rationale |
|---|---|
| Fjall internal file layout is not documented | Fjall owns it; NervusDB documents logical keyspaces only |
| No property range index in 0.1 | Correct graph persistence matters before planner/index breadth |
| No independent edge ID in 0.1 | `(src, rel, dst)` is enough for the core embedded graph use case |

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
| Query/storage dependency | 010-fjall-storage-refactor | `nervusdb-query` no longer depends on `nervusdb-storage`; writes go through `nervusdb-api::WriteableGraph` |
| Old facade path semantics | 010-fjall-storage-refactor | `Db::open(path)` opens a database directory; `open_paths`, `ndb_path`, and `wal_path` were removed |
| Old storage engine | 010-fjall-storage-refactor | Pager, WAL, B+Tree, CSR, L0 runs, overlay, and old read-path files were deleted |
| Label scan path | 010-fjall-storage-refactor | `GraphSnapshot::nodes_with_label` and Fjall `label_nodes` keyspace are implemented |
| Label/reltype namespace | 010-fjall-storage-refactor | Labels and reltypes use separate Fjall keyspaces and independent counters |
| Core query scope drift in 0.1 gates | 010-fjall-storage-refactor | Core tests/examples no longer require optional match, aggregation, ordering/skip, or index backfill |

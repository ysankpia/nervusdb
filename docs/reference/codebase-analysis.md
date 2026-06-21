# Codebase Analysis

Generated from CodeGraph exploration on 2026-06-14. This document is a
historical snapshot from before ADR 0005. It does not define current scope or
architecture.

Current direction lives in:

- `docs/decisions/0005-fjall-storage-backend.md`
- `docs/plans/active/010-fjall-storage-refactor.md`
- `docs/architecture/storage-model.md`
- `docs/reference/storage-format.md`

Use the material below only as evidence of the old pre-Fjall codebase shape.

## Contents

1. [Project Overview](#1-project-overview)
2. [Workspace Structure](#2-workspace-structure)
3. [Crate Analysis](#3-crate-analysis)
4. [Test Landscape](#4-test-landscape)
5. [Script Landscape](#5-script-landscape)
6. [CI Landscape](#6-ci-landscape)
7. [Pain Points](#7-pain-points)
8. [Recommended Next Steps](#8-recommended-next-steps)

---

## 1. Project Overview

**Product**: SQLite for property graphs — Rust-first embedded graph database.
**Phase at capture time**: Refactoring from platform-era breadth toward a
finishable 0.1 core.
**Workspace at capture time**: 7 crates, ~11 KLOC core code, ~40 KLOC total
(including tests).
**Default validation**: `bash scripts/check.sh` — fmt + core clippy + Mini-Cypher
9-test suite.
**CI**: `ci.yml` runs default validation on push/PR to `main`.

---

## 2. Workspace Structure

```
Cargo.toml (workspace members = 7)
├── nervusdb           — public Rust facade
├── nervusdb-api       — boundary traits (GraphSnapshot, GraphStore)
├── nervusdb-storage   — page store, WAL, recovery, labels, properties, indexes
├── nervusdb-query     — Mini-Cypher parser/planner/executor + full openCypher
├── nervusdb-cli       — CLI tool (query/write/repl/vacuum)
├── nervusdb-capi      — C ABI bindings (experimental)
└── nervusdb-pyo3      — Python bindings (experimental)
```

**External workspace members** (in repo but not in `Cargo.toml` members):
- `nervusdb-node/` — Node.js N-API bindings (experimental)

### Dependency Graph (Core)

```
nervusdb-cli → nervusdb → nervusdb-storage
                          → nervusdb-query → nervusdb-api
                          → nervusdb-api

nervusdb-storage → nervusdb-api
```

**Observation**: `nervusdb-api` is the only boundary between storage and query.
Neither `nervusdb-storage` nor `nervusdb-query` depends on the other directly.
This is a clean separation.

---

## 3. Crate Analysis

### 3.1 `nervusdb-api`

| Aspect | Detail |
|---|---|
| Path | `nervusdb-api/src/lib.rs` |
| Core types | `PropertyValue` (9-variant enum), `EdgeKey`, `InternalNodeId`, `LabelId`, `RelTypeId` |
| Core traits | `GraphSnapshot` (read), `GraphStore` (factory), `DbError`, `ReadWriteTxn` |
| Tests | None (no `tests/` directory) |
| Health | ✅ Stable, clean boundary. |

`GraphSnapshot` is implemented by `StorageSnapshot` in `nervusdb-storage`.
`PropertyValue` is used across all crates (104 callers in core code).

### 3.2 `nervusdb-storage`

| Aspect | Detail |
|---|---|
| Path | `nervusdb-storage/src/` — 39 source files |
| Core structs | `GraphEngine` (17 fields), `Pager`, `Wal`, `Snapshot`, `StorageSnapshot` |
| Core types | `PageId`, `EdgeKey`, `PropertyValue`, `L0Run`, `PublishedRuns`, `CsrSegment` |
| Key file: `engine.rs` | `GraphEngine::open()` orchestrates: Pager::open → Wal::open → WAL replay → label replay → graph replay → publish snapshots |
| Key file: `pager.rs` | `Pager` struct, page read/write/allocate/free, meta page management |
| Key file: `wal.rs` | `Wal` struct, append/persist/replay, `WalRecord` enum, `WalReader` |
| Key file: `snapshot.rs` | `Snapshot` (internal), `PublishedRuns`, neighbor iterators |
| Key file: `api.rs` | `StorageSnapshot` (implements `GraphSnapshot`), `GraphStore` impl for `GraphEngine` |
| Index code | `index/btree.rs` (B-tree), `index/hnsw/` (HNSW/vector), `index/catalog.rs` |
| Tests | `tests/` — 11 files (core_0_1_storage, m1_graph, m2_compaction, properties, tombstone, label, hnsw, btree, multi_label, snapshot, api_trait) |
| Health | ⚠️ HNSW/vector embedded with no feature gate. `engine.rs:open()` is a ~80-line monolith. |

**`GraphEngine::open()` flow**:
```
Pager::open → Wal::open → load IdMap → load IndexCatalog
→ init HNSW (always!) → WAL replay → scan recovery state
→ load CSR segments → replay labels → replay graph transactions
→ build snapshots → return GraphEngine
```

The HNSW initialization at `engine.rs:110-119` is **unconditional**. It creates
`__sys_hnsw_vec` and `__sys_hnsw_graph` index catalog entries and loads the full
HNSW index on every database open, even when the user never uses vector search.

### 3.3 `nervusdb-query`

| Aspect | Detail |
|---|---|
| Path | `nervusdb-query/src/` — 15 source files + evaluator/ + executor/ + query_api/ |
| Parser | `parser.rs` (1992 lines), `lexer.rs`, `parser_helper_exists.rs` |
| AST | `ast.rs` — `Query` → `Vec<Clause>` with 15+ clause variants |
| Executor | `executor.rs` + `executor/` (10+ plan files) — plan types include NodeScan, LabelScan, Neighbor, Filter, Project, Limit, Sort, Skip, CreateNode, CreateEdge, Delete, Set, Merge, Foreach, Subquery, Union, Call |
| Planner | `query_api/planner.rs` — LogicalPlan → PhysicalPlan → compile |
| Evaluator | `evaluator.rs` + `evaluator/` — expression evaluation, graph functions |
| Tests | Inline tests in `parser.rs`, plus facade-level tests through `nervusdb/tests/` |
| Health | ⚠️ Core Mini-Cypher path is clean, but full openCypher AST types, executor plans, and evaluator logic remain in the same crate with no feature isolation. |

**Mini-Cypher supported forms** (from `core_0_1_mini_cypher.rs`):
- `RETURN 1`
- `MATCH (n)`
- `MATCH (n:Label)` + property filter + LIMIT
- `MATCH (a)-[:TYPE]->(b)` (one-hop)
- `MATCH (a)-[:TYPE]->(b)-[:TYPE]->(c)` (two-hop)
- `CREATE (n:Label {props})`
- `SET n.prop = value`
- `DELETE n`
- `EXPLAIN`

**Historical openCypher code present** (not Mini-Cypher 0.1):
- `MERGE` (subclauses, merge_execute_support, merge_test.rs)
- `OPTIONAL MATCH` (t151_optional_match.rs)
- `WITH` (t305_with_clause.rs)
- `UNION` / `UNION ALL` (t307_union.rs)
- `UNWIND` (t306_unwind.rs)
- Aggregation (count, sum, avg, min, max, collect — t152_aggregation.rs)
- Subqueries (t319_subquery.rs)
- Pattern comprehension (t326_pattern_comprehension.rs)
- Procedures (t320_procedures.rs)
- `FOREACH` (t324_foreach.rs)
- Named paths (t334_named_path.rs)
- Variable-length paths (t60_variable_length_test.rs)
- `ORDER BY` / `SKIP` (t62_order_by_skip_test.rs)
- `CASE` expressions (t308_case_expr.rs)
- `EXISTS` (t309_exists.rs)
- String advanced (t302_string_advanced.rs)
- Complex types (t154_complex_types.rs)

### 3.4 `nervusdb` (Facade)

| Aspect | Detail |
|---|---|
| Path | `nervusdb/src/lib.rs` (776 lines), `nervusdb/src/error.rs` |
| Core API | `Db::open`, `Db::open_paths`, `Db::snapshot`, `Db::begin_read`, `Db::begin_write` |
| WriteTxn API | `get_or_create_label`, `get_or_create_rel_type`, `create_node`, `create_edge`, `set_node_property`, `set_edge_property`, `remove_node_property`, `remove_edge_property`, `tombstone_node`, `tombstone_edge`, `commit` |
| Exports | `PropertyValue`, `GraphSnapshot`, `GraphStore`, `PAGE_SIZE`, `query`, backup/vacuum/bulkload types |
| Tests (core) | `tests/core_0_1_rust_api.rs` |
| Tests (historical) | 40+ `tXXX_*.rs` files in `nervusdb/tests/` |
| Health | ✅ Core path documented and tested. ⚠️ Re-exports experimental API types (BackupHandle, BulkLoader, VacuumReport) at the top level. |

### 3.5 `nervusdb-cli`

| Aspect | Detail |
|---|---|
| Path | `nervusdb-cli/src/main.rs` (281 lines), `repl.rs` |
| Subcommands | `v2 query`, `v2 write`, `v2 repl`, `v2 vacuum` |
| Output | NDJSON |
| Health | ✅ Small, focused, well-scoped. |

### 3.6 `nervusdb-capi`

| Aspect | Detail |
|---|---|
| Path | `nervusdb-capi/src/lib.rs` |
| API surface | `ndb_open`, `ndb_open_paths`, `ndb_query`, `ndb_execute_write`, `ndb_txn_query`, `ndb_snapshot`, etc. |
| Tests | `tests/capi_smoke.rs` |
| Health | ⚠️ Experimental, in workspace, increases build surface. |

### 3.7 `nervusdb-pyo3`

| Aspect | Detail |
|---|---|
| Path | `nervusdb-pyo3/src/lib.rs` |
| API | `open`, `vacuum`, `backup`, `bulkload`, `Db`, `WriteTxn`, `QueryStream` |
| Tests | `tests/test_basic.py`, `tests/test_vector.py` |
| Health | ⚠️ Experimental, in workspace, Python 3.x build dependency. |

---

## 4. Test Landscape

### 4.1 Core 0.1 Tests (Default Validation Path)

| Test | Location | Type | What it proves |
|---|---|---|---|
| `core_0_1_return_one` | `nervusdb/tests/core_0_1_mini_cypher.rs` | Integration | `RETURN 1` |
| `core_0_1_match_all_nodes` | same file | Integration | `MATCH (n)` |
| `core_0_1_label_scan_property_filter_and_limit` | same file | Integration | `MATCH (n:Label {k: v}) ... LIMIT` |
| `core_0_1_one_hop_and_two_hop_traversal` | same file | Integration | One-hop and two-hop |
| `core_0_1_simple_string_and_integer_filters` | same file | Integration | Property equality |
| `core_0_1_create_edge_query_then_match` | same file | Integration | CREATE edge → MATCH |
| `core_0_1_basic_create_set_delete_and_explain` | same file | Integration | CREATE/SET/DELETE/EXPLAIN |
| `core_0_1_write_reopen_query_survives` | same file | Integration | reopen persistence |
| `core_0_1_limit_zero_and_limit_cap` | same file | Integration | LIMIT 0, LIMIT cap |
| `core_0_1_rust_api.rs` | `nervusdb/tests/` | Integration | Facade API baseline |
| `core_0_1_storage.rs` | `nervusdb-storage/tests/` | Integration | Storage baseline |

**Total**: 11 core test files. All run through `scripts/workspace_quick_test.sh`.

### 4.2 Historical Tests (Not Default Path)

`nervusdb/tests/` contains 50+ integration test files. Key categories:

| Category | Files | Count |
|---|---|---|
| Legacy openCypher features | `t151_optional_match` through `t343_parity_semantics` | ~35 |
| TCK harness | `tck_harness.rs` | 1 |
| Storage/misc | `create_test`, `filter_test`, `limit_boundary_test`, `merge_test`, `smoke`, `resilience_labels` | 6 |
| Core (listed above) | `core_0_1_*` | 3 |
| CLI import | `t202_import_cli.rs` | 1 |
| Query API | `t52_query_api.rs` | 1 |

`nervusdb-storage/tests/` contains 11 test files, of which only 1 is a 0.1 core
test (`core_0_1_storage.rs`).

### 4.3 Binding Tests (Manual)

| Test | Location |
|---|---|
| C API smoke | `nervusdb-capi/tests/capi_smoke.rs` |
| Python capabilities | `examples-test/nervusdb-python-test/test_capabilities.py` |
| Node.js capabilities | `examples-test/nervusdb-node-test/src/test-capabilities.ts` |
| Rust capabilities | `examples-test/nervusdb-rust-test/tests/test_capabilities.rs` |

### 4.4 Test Quality Assessment

- **Core coverage**: Adequate for the Mini-Cypher surface. 9 query forms tested.
  Missing: empty label scan, missing property, error paths, format epoch mismatch,
  concurrent read isolation.
- **Storage coverage**: Good baseline. Reopen, recovery, label interning,
  tombstone semantics covered. Missing: format epoch fail-fast test.
- **Historical coverage**: Extensive but unused. ~35 test files for full openCypher
  that are irrelevant to 0.1.
- **Binding coverage**: Exists but manual-only.

---

## 5. Script Landscape

| Script | Core? | Default? | Purpose |
|---|---|---|---|
| `scripts/check.sh` | ✅ | ✅ Default | fmt + core clippy + quick test |
| `scripts/workspace_quick_test.sh` | ✅ | ✅ (via check) | Core Mini-Cypher test |
| `scripts/workspace_full_test.sh` | ✅ | ❌ Manual | Full workspace (excl. pyo3 + TCK) |
| `scripts/core_smoke.sh` | ✅ | ❌ Manual | CLI smoke |
| `scripts/core_crash_recovery.sh` | ✅ | ❌ Manual | Crash recovery |
| `scripts/core_bench.sh` | ✅ | ❌ Manual | Benchmark |
| `scripts/core_examples.sh` | ✅ | ❌ Manual | 10 CLI examples |
| `scripts/tck_*.sh` (5 scripts) | ❌ | ❌ Manual | TCK compatibility |
| `scripts/binding_smoke.sh` | ❌ | ❌ Manual | Binding smoke |
| `scripts/binding_parity_gate.sh` | ❌ | ❌ Manual | Binding parity |
| `scripts/perf_slo_*.sh` (3 scripts) | ❌ | ❌ Manual | Performance |
| `scripts/chaos_io_gate.sh` | ❌ | ❌ Manual | Chaos testing |
| `scripts/soak_stability.sh` | ❌ | ❌ Manual | Soak testing |
| `scripts/fuzz_regress.sh` | ❌ | ❌ Manual | Fuzz regression |
| `scripts/stability_window.sh` | ❌ | ❌ Manual | Stability window |
| `scripts/hnsw_tune.sh` | ❌ | ❌ Manual | HNSW tuning |
| `scripts/release.sh` | ❌ | ❌ Manual | Release automation |
| `scripts/contract_smoke.sh` | ❌ | ❌ Manual | Contract compatibility |
| `scripts/parity_*_audit.sh` (2 scripts) | ❌ | ❌ Manual | Parity audit |
| `scripts/abi_binding_dep_gate.sh` | ❌ | ❌ Manual | ABI dependency |
| `scripts/concurrency_profile.sh` | ❌ | ❌ Manual | Concurrency profiling |
| `scripts/v2_bench.sh` | ❌ | ❌ Manual | v2 benchmark |
| `scripts/beta_*.sh` (2 scripts) | ❌ | ❌ Manual | Beta release |
| `scripts/benchmark_compare.sh` | ❌ | ❌ Manual | Benchmark comparison |

**Total**: 36 scripts. **Core default**: 1 (`check.sh` which chains to
`workspace_quick_test.sh`). **Core manual**: 4. **Historical manual**: 31.

---

## 6. CI Landscape

| Workflow | Trigger | What it runs | Status |
|---|---|---|---|
| `ci.yml` | push/PR to main | `bash scripts/check.sh` | ✅ Default |
| `ci-daily-snapshot.yml` | daily | Full workspace | ❌ Nightly |
| `crash-gate-v2.yml` | manual | Crash recovery | ❌ Manual trigger |
| `benchmark-nightly.yml` | nightly | Benchmark | ❌ Nightly |
| `chaos-nightly.yml` | nightly | Chaos | ❌ Nightly |
| `fuzz-nightly.yml` | nightly | Fuzz | ❌ Nightly |
| `perf-slo-nightly.yml` | nightly | Performance SLO | ❌ Nightly |
| `soak-nightly.yml` | nightly | Soak | ❌ Nightly |
| `stability-window-daily.yml` | daily | Stability window | ❌ Daily |
| `tck-nightly.yml` | nightly | TCK | ❌ Nightly |
| `release.yml` | tag | Release | ❌ Tag |

**Total**: 11 workflows. **Default**: 1. **Nightly/daily/manual**: 10.

---

## 7. Pain Points

### 🟥 High Priority

#### 7.1 HNSW/Vector unconditionally initialized in storage

**Location**: `nervusdb-storage/src/engine.rs:110-119`

```rust
// Initialize HNSW Index (T203)
let vec_def = index_catalog.get_or_create(&mut pager, "__sys_hnsw_vec")?;
let graph_def = index_catalog.get_or_create(&mut pager, "__sys_hnsw_graph")?;
let v_store = PersistentVectorStorage::new(BTree::load(vec_def.root));
let g_store = PersistentGraphStorage::new(BTree::load(graph_def.root));
let params = load_hnsw_params_from_env();
let vector_index = HnswIndex::load(params, v_store, g_store, &mut pager)?;
```

Every `Db::open` allocates HNSW index catalog entries and a `NativeHnsw` in
`GraphEngine`, even when the user never touches vector search. This pulls in
`rand`, `ordered-float` and other dependencies.

**Fix**: Feature gate `#[cfg(feature = "hnsw")]` on the HNSW initialization block
and `NativeHnsw` field.

#### 7.2 Experimental bindings in workspace

**Impact**:
- `cargo build --workspace` builds `nervusdb-capi` and `nervusdb-pyo3`, requiring
  a C compiler and Python 3.x.
- `cargo test --workspace` attempts to compile and run binding tests.
- `nervusdb-pyo3` depends on `pyo3` which adds build complexity.

**Fix**: Remove from `[workspace.members]` in `Cargo.toml`, or add `exclude` for
post-0.1 consideration.

#### 7.3 Full openCypher code in query crate without isolation

**Impact**:
- `nervusdb-query/src/ast.rs` has 15+ clause variants, only ~6 are Mini-Cypher.
- `nervusdb-query/src/executor/` has plan types (Merge, Foreach, Subquery, Union,
  Call) unused by 0.1.
- `nervusdb/tests/` has ~35 legacy test files that test full openCypher.

**Fix**: Feature gate `#[cfg(feature = "full-cypher")]` on non-Mini-Cypher clause
variants, executor plans, and test files.

### 🟨 Medium Priority

#### 7.4 57 of 60 integration tests are historical

Only 3 of 60 files in `nervusdb/tests/` are 0.1 core tests. The remaining 57 test
full openCypher features that are explicitly out of scope (ADR 0002, 0004).

**Impact**: `cargo test -p nervusdb` without filtering runs the full historical
suite. This is confusing for new contributors.

**Fix**: Annotate test files with `#[cfg(feature = "full-cypher")]` or move to
a separate directory.

#### 7.5 31 of 36 scripts are not default-loop

**Impact**: Cognitive load. Contributors must learn which scripts are active.

**Fix**: Reorganize `scripts/` into subdirectories: `core/`, `experimental/`,
`archive/`.

#### 7.6 `nervusdb-query/src/parser.rs` is 1992 lines

**Impact**: Single-file parser with ~2000 lines including tests. Hard to navigate.

**Fix**: Extract tests into a separate file. Consider module decomposition.

#### 7.7 `GraphEngine::open()` is a monolith (~80 lines)

**Impact**: Hard to unit-test recovery path independently.

**Fix**: Extract WAL recovery, label replay, and graph replay into named
functions.

### 🟩 Low Priority

#### 7.8 HNSW full implementation in storage

`nervusdb-storage/src/index/hnsw/` contains a complete HNSW implementation
(logic.rs, storage.rs) with ~650 lines. This is not needed for 0.1.

#### 7.9 Experimental API types re-exported from facade

`nervusdb/src/lib.rs` re-exports `BackupHandle`, `BulkLoader`, `VacuumReport`
at the crate root. These are not 0.1 core API.

#### 7.10 `PropertyValue::DateTime` and `Blob` variants

These exist in `nervusdb-api` but Mini-Cypher 0.1 does not support them. They
can remain as API types but should not appear in Mini-Cypher tests.

---

## 8. Recommended Next Steps

### Phase A: Feature Isolation (estimated: 1-2 days)

```
Step 1: Storage HNSW feature gate
  └── engine.rs: wrap HNSW init in #[cfg(feature = "hnsw")]
  └── engine.rs: Option<NativeHnsw> → wrap in conditional
  └── Cargo.toml: add "hnsw" feature, default = ["hnsw"] (opt-out)
  └── verify: bash scripts/check.sh passes
  └── verify: cargo build --no-default-features excludes HNSW

Step 2: Query openCypher feature gate
  └── Cargo.toml: add "full-cypher" feature
  └── parser.rs: conditionally compile full-Cypher clause parsing
  └── executor/: conditionally compile non-Mini-Cypher plan types
  └── verify: 9 Mini-Cypher tests still pass
```

### Phase B: Test Cleanup (estimated: 1-2 days)

```
Step 3: Mark historical tests as conditional
  └── Annotate non-core test files with #[cfg(feature = "full-cypher")]
  └── Cargo.toml [[test]] for core vs historical

Step 4: Add missing core tests
  └── Format epoch mismatch fail-fast
  └── Empty label scan (MATCH (n:NonExistent))
  └── LIMIT 0 with various clauses
  └── Node with no properties
  └── Edge with no properties
```

### Phase C: Engineering Cleanup (estimated: 2-3 days)

```
Step 5: Reorganize scripts
  └── scripts/core/ → default + manual core scripts
  └── scripts/experimental/ → historical scripts
  └── scripts/archive/ → clearly deprecated

Step 6: Remove experimental bindings from workspace
  └── Cargo.toml: [workspace.exclude] for nervusdb-capi, nervusdb-pyo3
  └── Or move to [workspace] exclude

Step 7: Extract parser tests
  └── Move inline parser tests to parser_test.rs
  └── Reduce parser.rs to ~1200 lines

Step 8: Break up GraphEngine::open()
  └── Extract recover_wal(), replay_labels(), replay_graph()
  └── Add unit tests for each
```

### Phase D: 0.1 Hardening (estimated: 3-5 days)

```
Step 9: Storage hardening
  └── CRC32 checksum on pages
  └── Format epoch mismatch unit test
  └── Strict WAL format validation

Step 10: Crash recovery batch test
  └── Automate multi-round kill/reopen/verify
  └── Test with concurrent reads during recovery

Step 11: Large smoke
  └── bash scripts/core_bench.sh --large
  └── Record hardware, scale, P50/P95/P99

Step 12: Release preparation
  └── CHANGELOG update
  └── Version bump to 0.1.0-rc.1
  └── crates.io publish validation
```

---

## Exploration Commands

The data for this analysis was collected with:

```bash
codegraph explore "nervusdb workspace Cargo.toml crate members dependency graph full"
codegraph explore "nervusdb storage layer architecture engine pager wal recovery snapshot"
codegraph explore "nervusdb query layer parser planner executor mini cypher"
codegraph explore "nervusdb facade Db open snapshot write public API"
codegraph explore "nervusdb CLI main command v2 query write repl import"
codegraph explore "nervusdb tests integration core mini cypher rust api storage"
codegraph explore "nervusdb scripts check.sh workspace core crash recovery benchmark examples"
codegraph explore "nervusdb api GraphSnapshot trait property value"
codegraph node nervusdb/src/lib.rs
codegraph node nervusdb-cli/src/main.rs
```

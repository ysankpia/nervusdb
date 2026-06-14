# Slimming Plan: Cut To Ship 0.1

## Problem

NervusDB 目前想做的太多 — 一个嵌套在文件里的「完整图平台」：
HNSW 向量搜索、Python/Node.js/C 三种绑定、完整 openCypher（MERGE/WITH/UNION/
子查询/聚合/模式推导）、40 个集成测试、36 个脚本、11 个 CI workflow。

**SQLite 能成功，是因为它只做一件事，且做得极其无聊。**

NervusDB 0.1 需要的是：Rust crate 打开一个文件，写图数据，按标签扫描，
按关系类型遍历邻居，崩溃后能恢复。仅此而已。

## 必须删除的内容

按删除后直接减少了多少代码量排序。

### 1. HNSW/向量搜索 — 删 824 行 + 3 个外部依赖

```
git rm -r nervusdb-storage/src/index/hnsw/
```

**删除文件**:
- `nervusdb-storage/src/index/hnsw/mod.rs`
- `nervusdb-storage/src/index/hnsw/logic.rs` (292 行 HNSW 算法)
- `nervusdb-storage/src/index/hnsw/storage.rs` (505 行持久化)
- `nervusdb-storage/src/index/hnsw/params.rs` (19 行参数)
- `nervusdb-storage/src/index/hnsw/dist.rs` (1 行距离)

**连带删除**:
- `nervusdb-storage/src/index/vector.rs` (77 行 BruteForceIndex)
- `nervusdb-storage/tests/t203_hnsw.rs` (HNSW 测试)

**修改文件**:
- `nervusdb-storage/src/engine.rs:110-119` — 删除无条件 HNSW 初始化
- `nervusdb-storage/src/engine.rs` — 删除 `vector_index` 字段 (Arc<Mutex<NativeHnsw>>)
- `nervusdb-storage/src/engine.rs` — 删除 `with_catalog_pager_vector()` 方法
- `nervusdb-storage/src/lib.rs` — 删除 `pub mod index` 中的 hnsw 导出 (或保留 btree 即可)
- `nervusdb-storage/Cargo.toml` — 删除 `rand`, `ordered-float` 依赖

**效果**: core 依赖减少 2 个，open 路径减少 ~50 行条件分支。

### 2. Python 绑定 — 删 ~2000 行 (估算)

```
git rm -rf nervusdb-pyo3/
```

**删除文件**: `nervusdb-pyo3/src/lib.rs`, `tests/`, `Cargo.toml` 等全部

**连带删除**:
- `examples-test/nervusdb-python-test/` (Python 能力测试)

### 3. C API 绑定 — 删 ~1500 行 (估算)

```
git rm -rf nervusdb-capi/
```

**删除文件**: `nervusdb-capi/src/lib.rs` (~600 行 C ABI 包装), `tests/capi_smoke.rs`,
`Cargo.toml`, `build.rs` 等

### 4. Node.js 绑定 — 删 ~1500 行 (估算)

```
git rm -rf nervusdb-node/
```

**删除文件**: `nervusdb-node/src/lib.rs`, `index.ts`, `Cargo.toml`, `package.json` 等

**连带删除**:
- `examples-test/nervusdb-node-test/` (Node.js 能力测试)
- `examples-test/nervusdb-rust-test/` (如果只测绑定的 Rust 测试)

**效果**: workspace 从 7 个 crate 减到 4 个（nervusdb, nervusdb-api, nervusdb-storage, nervusdb-query, nervusdb-cli）。`cargo build --workspace` 不再需要 C 编译器和 Python 3.x。

### 5. 历史集成测试 — 删 ~35 个文件

```
git rm nervusdb/tests/t151_optional_match.rs
git rm nervusdb/tests/t152_aggregation.rs
git rm nervusdb/tests/t153_varlen_optional.rs
git rm nervusdb/tests/t154_complex_types.rs
git rm nervusdb/tests/t155_edge_persistence.rs
git rm nervusdb/tests/t155_overflow.rs
git rm nervusdb/tests/t156_optimizer.rs
git rm nervusdb/tests/t202_import_cli.rs
git rm nervusdb/tests/t301_expression_ops.rs
git rm nervusdb/tests/t302_string_advanced.rs
git rm nervusdb/tests/t304_remove_clause.rs
git rm nervusdb/tests/t305_with_clause.rs
git rm nervusdb/tests/t306_unwind.rs
git rm nervusdb/tests/t307_union.rs
git rm nervusdb/tests/t308_case_expr.rs
git rm nervusdb/tests/t309_exists.rs
git rm nervusdb/tests/t311_expressions.rs
git rm nervusdb/tests/t312_unary.rs
git rm nervusdb/tests/t313_functions.rs
git rm nervusdb/tests/t314_pattern_general.rs
git rm nervusdb/tests/t315_direction.rs
git rm nervusdb/tests/t316_type_alternation.rs
git rm nervusdb/tests/t317_joins.rs
git rm nervusdb/tests/t318_paths.rs
git rm nervusdb/tests/t319_subquery.rs
git rm nervusdb/tests/t320_procedures.rs
git rm nervusdb/tests/t321_incoming.rs
git rm nervusdb/tests/t323_merge_semantics.rs
git rm nervusdb/tests/t324_foreach.rs
git rm nervusdb/tests/t325_pattern_props.rs
git rm nervusdb/tests/t326_pattern_comprehension.rs
git rm nervusdb/tests/t332_binding_validation.rs
git rm nervusdb/tests/t333_varlen_direction.rs
git rm nervusdb/tests/t334_named_path.rs
git rm nervusdb/tests/t335_graph_label_expression.rs
git rm nervusdb/tests/t336_return_orderby_scope.rs
git rm nervusdb/tests/t341_resource_limits.rs
git rm nervusdb/tests/t342_label_merge_regressions.rs
git rm nervusdb/tests/t343_parity_semantics.rs
git rm nervusdb/tests/t52_query_api.rs
git rm nervusdb/tests/t53_integration_storage.rs
git rm nervusdb/tests/t60_variable_length_test.rs
git rm nervusdb/tests/t62_order_by_skip_test.rs
git rm nervusdb/tests/t64_node_scan_test.rs
git rm nervusdb/tests/t104_explain_test.rs
git rm nervusdb/tests/t105_merge_test.rs
git rm nervusdb/tests/t106_checkpoint_on_close.rs
git rm nervusdb/tests/t107_index_integration.rs
git rm nervusdb/tests/t108_set_clause.rs
git rm nervusdb/tests/create_test.rs
git rm nervusdb/tests/filter_test.rs
git rm nervusdb/tests/limit_boundary_test.rs
git rm nervusdb/tests/merge_test.rs
git rm nervusdb/tests/smoke.rs
git rm nervusdb/tests/resilience_labels.rs
git rm nervusdb/tests/fuzz_cypher.rs
git rm nervusdb/tests/tck_harness.rs
git rm -rf nervusdb/tests/opencypher_tck/
git rm nervusdb/tests/fuzz_cypher.proptest-regressions
```

**保留**:
- `nervusdb/tests/core_0_1_mini_cypher.rs` ✅ — 9 个核心测试
- `nervusdb/tests/core_0_1_rust_api.rs` ✅ — Facade API 测试

**Storage 测试清理** — 删除非核心:
```
git rm nervusdb-storage/tests/m1_graph.rs
git rm nervusdb-storage/tests/m2_compaction.rs
git rm nervusdb-storage/tests/t203_hnsw.rs
git rm nervusdb-storage/tests/t206_btree_delete.rs
git rm nervusdb-storage/tests/t322_multi_label.rs
git rm nervusdb-storage/tests/t47_api_trait.rs
git rm nervusdb-storage/tests/t51_snapshot_scan.rs
git rm nervusdb-storage/tests/t59_label_interning.rs
git rm nervusdb-storage/tests/properties.rs
git rm nervusdb-storage/tests/tombstone_semantics.rs
```
**保留**: `nervusdb-storage/tests/core_0_1_storage.rs` ✅

**效果**: `cargo test -p nervusdb` 从 ~60 个测试文件减少到 2 个。
`cargo test -p nervusdb-storage` 从 11 个测试文件减少到 1 个。

### 6. 非 Mini-Cypher 查询解析逻辑

**AST 删减** (`nervusdb-query/src/ast.rs`):
- 删除 `Merge(MergeClause)` — MERGE 是完整 Cypher 特性
- 删除 `Unwind(UnwindClause)` — UNWIND 不是 Mini-Cypher
- 删除 `Call(CallClause)` — 子查询/存储过程
- 删除 `With(WithClause)` — WITH 不是 0.1 必须
- 删除 `Union(UnionClause)` — UNION 不是 Mini-Cypher
- 删除 `Foreach(ForeachClause)` — FOREACH 不是 Mini-Cypher
- `MatchClause.optional` 字段 — OPTIONAL MATCH 不删 parser，去掉 optional flag
- `ReturnClause.order_by`/`skip`/`distinct` — ORDER BY / SKIP / DISTINCT 不删解析，去掉 executor 执行

**Executor 删除**:
```
git rm nervusdb-query/src/executor/foreach_ops.rs
git rm nervusdb-query/src/executor/merge_execute_support.rs
git rm nervusdb-query/src/executor/merge_execution.rs
git rm nervusdb-query/src/executor/merge_helpers.rs
git rm nervusdb-query/src/executor/merge_overlay.rs
git rm nervusdb-query/src/executor/path_usage.rs
git rm nervusdb-query/src/executor/index_seek_plan.rs
git rm nervusdb-query/src/executor/join_apply.rs
git rm nervusdb-query/src/executor/match_in_undirected_plan.rs
git rm nervusdb-query/src/executor/procedure_registry.rs
git rm nervusdb-query/src/executor/binding_utils.rs
git rm nervusdb-query/src/executor/projection_sort.rs
git rm nervusdb-query/src/executor/runtime_limits.rs
git rm nervusdb-query/src/executor/write_orchestration.rs
git rm nervusdb-query/src/executor/write_support.rs
```

**Evaluator 删除**:
```
git rm nervusdb-query/src/evaluator/evaluator_temporal_parse.rs
```

**Parser 测试删除** (`nervusdb-query/src/parser.rs` 中的测试):
- 删除 `rejects_mixed_union_and_union_all` 等 ~50 个非核心 parser 测试
- 只保留验证合法 Mini-Cypher 输入的测试

### 7. CI Workflows — 删 10 个

```
git rm .github/workflows/ci-daily-snapshot.yml
git rm .github/workflows/crash-gate-v2.yml
git rm .github/workflows/benchmark-nightly.yml
git rm .github/workflows/chaos-nightly.yml
git rm .github/workflows/fuzz-nightly.yml
git rm .github/workflows/perf-slo-nightly.yml
git rm .github/workflows/soak-nightly.yml
git rm .github/workflows/stability-window-daily.yml
git rm .github/workflows/tck-nightly.yml
git rm .github/workflows/release.yml
```

**保留**: `.github/workflows/ci.yml` ✅

### 8. 验证脚本 — 删 31 个

```
git rm scripts/tck_smoke_gate.sh
git rm scripts/tck_tier_gate.sh
git rm scripts/tck_failure_cluster.sh
git rm scripts/tck_full_rate.sh
git rm scripts/tck_whitelist/ -rf
git rm scripts/binding_smoke.sh
git rm scripts/binding_parity_gate.sh
git rm scripts/perf_slo_gate.sh
git rm scripts/perf_slo_summary.sh
git rm scripts/perf_slo_window.sh
git rm scripts/chaos_io_gate.sh
git rm scripts/soak_stability.sh
git rm scripts/fuzz_regress.sh
git rm scripts/stability_window.sh
git rm scripts/hnsw_tune.sh
git rm scripts/release.sh
git rm scripts/contract_smoke.sh
git rm scripts/parity_coverage_audit.sh
git rm scripts/parity_softgate_audit.sh
git rm scripts/abi_binding_dep_gate.sh
git rm scripts/concurrency_profile.sh
git rm scripts/v2_bench.sh
git rm scripts/beta_gate.sh
git rm scripts/beta_release_gate.sh
git rm scripts/benchmark_compare.sh
git rm scripts/workspace_full_test.sh
git rm scripts/tests/ -rf
git rm scripts/git-hooks/ -rf
```

**保留**:
- `scripts/check.sh` ✅ — fmt + clippy + quick test
- `scripts/workspace_quick_test.sh` ✅ — 核心 Mini-Cypher 测试
- `scripts/core_smoke.sh` ✅ — CLI smoke
- `scripts/core_crash_recovery.sh` ✅ — 崩溃恢复
- `scripts/core_bench.sh` ✅ — 基准测试
- `scripts/core_examples.sh` ✅ — 10 个 CLI 示例

### 9. 实验性 Facade 导出

`nervusdb/src/lib.rs` 修改:
- 删除 `pub use nervusdb_storage::backup::{...}` — BackupManager 不是 0.1 API
- 删除 `pub use nervusdb_storage::bulkload::{...}` — BulkLoader 不是 0.1 API
- 删除 `pub use nervusdb_storage::vacuum::VacuumReport` — Vacuum 不是 0.1 API
- 选项: 保留 `Db::compact`, `Db::checkpoint`, `Db::close`, `Db::create_index`,
  `Db::search_vector` 但标注 `#[doc(hidden)]`

### 10. 示例目录清理

```
git rm -rf examples/py-local/
git rm -rf examples/ts-local/
```

**保留**: `examples/core-0.1/` ✅ — 10 个核心 0.1 示例

### 11. 其他杂物

```
git rm -rf fuzz/                          # fuzz targets, 非核心
git rm -rf artifacts/                     # 生成产物
git rm Makefile                           # 用 scripts/ 就够了
git rm lefthook.yml                       # 用 scripts/git-hooks/ 就够了（删除 hooks 后一并删除）
git rm .repomixignore repomix.config.json  # 外部工具配置
```

---

## 删除后效果

| 指标 | 当前 | 删除后 |
|---|---|---|
| workspace crates | 7 | **4** (nervusdb, api, storage, query, cli) |
| 集成测试文件 | ~60 | **2** |
| 验证脚本 | 36 | **6** |
| CI workflows | 11 | **1** |
| 外部 crate 依赖 (storage) | ~15 | **~13** (去掉 rand, ordered-float) |
| `cargo build` 是否需要 C 编译器 | 是 (pyo3) | **否** |
| 项目根目录条目 | ~43 | **~15** |
| 核心 Rust 代码 (估算) | ~11 KLOC | **~8 KLOC** |

---

## 0.1 最终形态

```
nervusdb/
├── nervusdb/               — Rust facade
│   └── tests/
│       ├── core_0_1_mini_cypher.rs  (9 个测试)
│       └── core_0_1_rust_api.rs
├── nervusdb-api/           — 边界 traits
├── nervusdb-storage/       — 页存储 + WAL + 恢复
│   └── tests/
│       └── core_0_1_storage.rs
├── nervusdb-query/         — Mini-Cypher 解析/执行
├── nervusdb-cli/           — CLI 工具
├── examples/
│   └── core-0.1/           — 10 个可运行示例
├── scripts/
│   ├── check.sh
│   ├── workspace_quick_test.sh
│   ├── core_smoke.sh
│   ├── core_crash_recovery.sh
│   ├── core_bench.sh
│   └── core_examples.sh
├── docs/                   — 文档
└── Cargo.toml              — 4 个 workspace member
```

---

## 执行顺序

1. **备份**: `git checkout -b chore/slim-to-0.1`
2. **删 HNSW**: 文件级删除，修改 engine.rs，去掉依赖
3. **删绑定**: pyo3 + capi + node + examples-test
4. **删测试**: 历史集成测试文件
5. **删 CI**: 10 个 workflow
6. **删脚本**: 31 个脚本
7. **删面代码**: 非 Mini-Cypher 的 AST 变体 + executor 文件
8. **清理 facade 导出**: backup/bulkload/vacuum
9. **清理示例**: py-local, ts-local
10. **清理杂物**: fuzz, artifacts, Makefile, lefthook
11. **验证**: `bash scripts/check.sh`
12. **提交**: 大提交，message 写清删除了什么
13. **编写新的方向契约**: 更新 docs 反映新的、更激进的范围

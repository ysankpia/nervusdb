# NervusDB v2 代码审查报告（2025-12-28，工作区 commit 87997e40）

生成方式：
- 使用 `repomix` 打包（outputId: `1ccfdcb0aac4c5f9`，排除 `_legacy_v1_archive/` 与 `target/`）做静态审查
- 以 `git ls-files` 的真实文件清单为准（本次审查范围：139 个文件）

## 0) Linus 的三问（先别自嗨）

1. **这是现实问题还是臆想？**  
   现实问题：你之前同时维护 v1/redb 与 v2/storage，精神分裂。已通过“v1 归档 + v2-only”解决。
2. **有没有更简单的办法？**  
   有：把“支持矩阵”收口到 v2 CLI 路径，所有超范围语法 fail-fast，别再假装兼容 Neo4j。
3. **会不会破坏任何东西？**  
   你说仓库开源两天、无用户，所以可以破坏。但**仍要避免“静默错误”**：不支持的语法必须报错，不能悄悄忽略。

## 【Core Judgment】

✅ 值得收尾：你现在的 v2 内核（pager/WAL/snapshot/compaction）和最小查询/CLI 路径已经构成“可用闭环”。  
❌ 不值得继续无限加语法：Cypher 全量兼容是泥潭，越写越烂、越写越慢、越写越不可能“结束”。

## 【Key Insights】

- **Data Structure**：v2 的核心数据流是 `MemTable(delta) -> L0Runs -> CSR segments`，一致性靠 `WAL + manifest/checkpoint`，读隔离靠 `Snapshot`。
- **Complexity**：最容易失控的是“查询语法覆盖率”；正确做法是白名单 + fail-fast，而不是堆 `if/else` 补洞。
- **Risk Point**：任何“解析了但忽略/不执行”的语义都是灾难（比如 `MATCH (n {prop:...})` 被当成普通 `MATCH`）。这会直接产生错误结果，比崩溃更烂。

## 【Taste Rating】

- 🟢 **v2-storage**：整体结构清晰，内核化思路正确（Pager/WAL/Manifest/Checkpoint 的边界能讲清）。  
- 🟡 **v2-query**：可用，但有明显“未来功能残留”痕迹（planner/部分算子未被 v2 API 路径使用，容易让支持矩阵漂移）。  
- 🟢 **CLI**：务实，NDJSON 输出很对路（可管道处理、可脚本化）。

## 【本次实际修正（防止“假支持”）】

- v2 可变长度路径真正落地到执行：`prepare()` 会为 `[:<u32>*min..max]` 生成 `MatchOutVarLen`，并对 `*` 缺省施加 hop 上限（避免无限遍历）。
- v2 `MATCH` 模式属性 fail-fast：`MATCH (n {name:'Alice'})` 和 `MATCH ()-[:1 {k:v}]->()` 这类 **以前会被静默忽略**，现在直接 `not implemented`（强制用户用 `WHERE`）。
- 清理无用测试/代码：删除“看起来像支持聚合但其实不算”的测试，避免误导。
- 文档对齐：`README.md`/`docs/spec.md`/`docs/reference/cypher_support.md` 与当前 v2-only 仓库事实一致；v1 发布/性能文档移入 `_legacy_v1_archive/`。

## 1) 你现在的 MVP 交付路径（别再漂移）

- 规格：`docs/spec.md`
- 完成标准：`docs/memos/DONE.md`
- 支持矩阵：`docs/reference/cypher_support.md`
- 验收命令：`cargo run -p nervusdb-cli -- v2 write/query ...`

## 2) 主要技术风险（按优先级）

1. **可变长度路径的“爆炸”风险（Medium）**  
   已通过默认 hop 上限缓解，但仍可能产生大量结果（尤其是高出度图）。如果你以后开放 `*..` 真无限，你就是在自杀。
2. **Query API 与 Planner 并存（Low/Medium）**  
   `nervusdb-v2-query/src/planner.rs` 不是当前 `prepare()` 的执行路径。要么删掉/归档，要么明确“planner 是未来，不属于 MVP”，否则文档必漂。
3. **Clippy 警告（Low）**  
   目前主要是 `type_complexity`/`too_many_arguments` 这种“品味问题”，不影响正确性；但别让它演变成“没人敢改”。

## 3) 每个文件的作用（逐文件一行）

> 说明：以下清单覆盖本次审查范围（排除 `_legacy_v1_archive/` 与 `target/`）。

### 3.1 仓库根目录

- `Cargo.toml`: Rust workspace 成员列表（v2-only）。
- `Cargo.lock`: 依赖锁定（可重复构建）。
- `README.md`: v2-only 项目入口与 5 分钟验收路径。
- `CHANGELOG.md`: v2 变更日志入口（v1 见归档）。
- `LICENSE`: Apache-2.0 许可证。
- `COMMERCIAL_LICENSE.md`: 商业许可条款（如适用）。
- `AGENTS.md`: 开发流程规范（spec/task/checklist）。
- `CLAUDE.md`: 同 `AGENTS.md`（工具链约束）。
- `GEMINI.md`: 同 `AGENTS.md`（工具链约束）。
- `.gitignore`: Git 忽略规则。
- `.repomixignore`: repomix 打包忽略规则。
- `repomix.config.json`: repomix 打包配置。
- `cspell.config.cjs`: 拼写检查配置（目前含历史目录忽略）。
- `.prettierrc`: Prettier 配置（主要用于历史 JS/TS 部分）。
- `.prettierignore`: Prettier 忽略。
- `.lintstaged.cjs`: lint-staged 配置（提交前门禁）。

### 3.2 GitHub / CI

- `.github/pull_request_template.md`: PR 模板（强制说明影响面/验证）。
- `.github/workflows/ci.yml`: CI（Rust build/test 等）。
- `.github/workflows/crash-gate-v2.yml`: v2 crash gate 门禁（恢复一致性）。

### 3.3 Husky Hooks

- `.husky/pre-commit`: 提交前门禁（fmt/clippy/test 等）。
- `.husky/pre-push`: 推送前门禁（更重的检查）。

### 3.4 文档（docs）

- `docs/spec.md`: v2 产品规格（唯一真相来源）。
- `docs/tasks.md`: 历史任务跟踪（含 v1/v2 记录，偏“项目史”）。
- `docs/reference/project-structure.md`: 当前仓库结构与 crate 边界。
- `docs/reference/cypher_support.md`: v2 Cypher 白名单与 fail-fast 规则。
- `docs/perf/V2_BENCH.md`: v2 bench 与 perf gate 说明。
- `docs/perf/PERFORMANCE_ANALYSIS.md`: v2 性能说明入口（v1 已归档）。
- `docs/perf/v2/README.md`: v2 perf runs 记录说明。
- `docs/release/publishing.md`: v2 发布指南入口（绑定/v1 已归档）。
- `docs/product/spec.md`: 兼容旧链接的占位（指向 `docs/spec.md`）。

#### 3.4.1 docs/memos（备忘录）

- `docs/memos/DONE.md`: 完成标准（终点线）。
- `docs/memos/M2025-12-27-gap-analysis.md`: 历史 gap analysis（已标注 scope frozen 后可能过时）。
- `docs/memos/v2-next-steps.md`: v2 后续建议（可作为 backlog，但不属于 MVP）。
- `docs/memos/v2-status-assessment.md`: v2 状态评估（历史视角，含与 v1 对比）。
- `docs/memos/CODE_REVIEW_2025-12-27.md`: 旧的全仓库审查报告（包含 v1/绑定，现已过时）。
- `docs/memos/CODE_REVIEW_2025-12-28.md`: 本文件（v2-only 审查报告）。

#### 3.4.2 docs/design（设计文档）

> 这些文件多数是“决策记录/历史上下文”，不是当前 MVP 的功能承诺。

- `docs/design/T1-storage-perf-baseline.md`: v1 性能基线与问题定位（历史）。
- `docs/design/T2-drop-synapsedb-pages.md`: 历史存储格式清理计划（偏 v1/绑定）。
- `docs/design/T3-intern-lru.md`: v1 字典 LRU 设计记录（历史）。
- `docs/design/T4-node-bulk-resolve.md`: v1 Node 批量 resolve 优化记录（历史）。
- `docs/design/T5-fuck-off-test.md`: crash-kill 一致性验证设计（理念仍适用）。
- `docs/design/T6-ffi-freeze.md`: v1 C ABI 冻结记录（历史）。
- `docs/design/T7-node-thin-binding.md`: v1 Node 绑定收敛记录（历史）。
- `docs/design/T8-temporal-default-off.md`: v1 temporal feature gate 记录（历史）。
- `docs/design/T9-node-ci.md`: v1 Node CI 记录（历史）。
- `docs/design/T10-binary-row-iterator.md`: v1 stmt/row iterator 设计（历史）。
- `docs/design/T11-perf-report-refresh.md`: v1 性能报告方法论修正（历史）。
- `docs/design/T12-release-1.0-prep.md`: v1 发布准备（历史）。
- `docs/design/T13-node-statement-api.md`: v1 Node statement API（历史）。
- `docs/design/T14-release-v1.0.0.md`: v1.0.0 发布记录（历史）。
- `docs/design/T15-true-streaming.md`: v1 流式执行器记录（历史）。
- `docs/design/T17-true-streaming.md`: v1 Arc/迭代器生命周期记录（历史）。
- `docs/design/T18-node-property-optimization.md`: v1 Node 属性写入优化（历史）。
- `docs/design/T19-temporal-separation.md`: v1 temporal crate 分离（历史）。
- `docs/design/T20-storage-key-compression.md`: v1 redb key 压缩（历史）。
- `docs/design/T21-order-by-skip.md`: ORDER BY/SKIP 设计记录（部分理念可复用）。
- `docs/design/T22-aggregate-functions.md`: 聚合函数设计记录（当前 MVP 不承诺）。
- `docs/design/T23-with-clause.md`: WITH 设计记录（当前 MVP 不承诺）。
- `docs/design/T24-optional-match.md`: OPTIONAL MATCH 设计记录（当前 MVP 不承诺）。
- `docs/design/T25-merge.md`: MERGE 设计记录（当前 MVP 不承诺）。
- `docs/design/T26-variable-length-paths.md`: 变长路径设计记录（v2 已实现受限版本）。
- `docs/design/T27-union.md`: UNION 设计记录（当前 MVP 不承诺）。
- `docs/design/T28-built-in-functions.md`: 内置函数设计记录（当前 MVP 不承诺）。
- `docs/design/T29-case-when.md`: CASE WHEN 设计记录（当前 MVP 不承诺）。
- `docs/design/T30-exists-call-subqueries.md`: EXISTS/CALL 设计记录（当前 MVP 不承诺）。
- `docs/design/T31-list-literals-comprehensions.md`: 列表/推导式设计记录（当前 MVP 不承诺）。
- `docs/design/T32-cypher-unwind-distinct-collect.md`: UNWIND/DISTINCT/COLLECT 设计记录（部分已在 v2 实现）。
- `docs/design/T33-vector-and-fts.md`: v1 向量/FTS 设计记录（历史）。
- `docs/design/T34-index-acceleration.md`: v1 索引加速（历史）。
- `docs/design/T35-vector-topk-pushdown.md`: v1 向量 top-k 下推（历史）。
- `docs/design/T36-release-v1.0.3.md`: v1.0.3 发布记录（历史）。
- `docs/design/T37-uniffi-bindings.md`: v1 UniFFI 绑定记录（历史）。
- `docs/design/T38-node-contract-ci.md`: v1 Node 契约门禁（历史）。
- `docs/design/T39-rust-cli.md`: Rust CLI 设计记录（v2 CLI 已落地）。
- `docs/design/T40-v2-kernel-spec.md`: v2 内核 spec（Pager/WAL/Crash model）。
- `docs/design/T41-v2-workspace-and-crate-structure.md`: v2 workspace/crate 边界。
- `docs/design/T42-v2-m0-pager-wal.md`: v2 M0（Pager + WAL replay）。
- `docs/design/T43-v2-m1-idmap-memtable-snapshot.md`: v2 M1（IDMap/MemTable/Snapshot）。
- `docs/design/T44-v2-m2-csr-segments-and-compaction.md`: v2 M2（CSR + compaction）。
- `docs/design/T45-v2-durability-checkpoint-and-crash-model.md`: v2 durability/checkpoint/crash model。
- `docs/design/T46-v2-public-api-facade.md`: v2 facade（Db/Txn）API。
- `docs/design/T47-v2-query-storage-boundary.md`: v2 query/storage 边界（GraphSnapshot）。
- `docs/design/T48-v2-benchmark-and-perf-gate.md`: v2 bench/perf gate。
- `docs/design/T49-v2-crash-gate.md`: v2 crash gate。
- `docs/design/T50-v2-m3-query-crate.md`: v2 query crate（parser/planner 迁移策略）。
- `docs/design/T51-v2-m3-executor-mvp.md`: v2 executor MVP（pull-based）。
- `docs/design/T52-v2-m3-query-api.md`: v2 Query API（prepare/execute）。
- `docs/design/T53-v2-m3-query-tests.md`: v2 query 测试策略。
- `docs/design/T54-v2-property-storage.md`: v2 属性存储层设计。
- `docs/design/T56-v2-delete.md`: v2 DELETE/DETACH DELETE 设计。
- `docs/design/T57-v2.0.0-release.md`: v2.0.0 发布门槛与验收。
- `docs/design/T58-v2-query-facade.md`: v2 query facade（query_collect/QueryExt）。
- `docs/design/T59-v2-label-interning.md`: v2 label interning 设计与实现。
- `docs/design/T60-v2-variable-length-paths.md`: v2 变长路径实现与测试。
- `docs/design/T61-v2-aggregation.md`: v2 聚合设计记录（当前 MVP 不承诺）。
- `docs/design/T62-v2-order-by-skip.md`: v2 ORDER BY/SKIP/LIMIT 设计与实现。
- `docs/design/T63-v2-python-bindings.md`: v2 Python binding 设计记录（当前仓库已归档绑定）。

### 3.5 脚本（scripts）

- `scripts/v2_bench.sh`: v2 bench 一键运行入口。

### 3.6 Rust Crates

#### 3.6.1 `nervusdb-v2-api/`

- `nervusdb-v2-api/Cargo.toml`: v2 API crate 配置（trait 边界）。
- `nervusdb-v2-api/src/lib.rs`: `GraphStore/GraphSnapshot` trait 与 ID 类型定义（查询/存储唯一耦合点）。

#### 3.6.2 `nervusdb-v2-storage/`（内核）

- `nervusdb-v2-storage/Cargo.toml`: v2 storage crate 配置。
- `nervusdb-v2-storage/src/lib.rs`: storage crate 入口与模块导出。
- `nervusdb-v2-storage/src/error.rs`: storage 错误类型与 Result。
- `nervusdb-v2-storage/src/pager.rs`: page store（8KB）+ 分配/读写。
- `nervusdb-v2-storage/src/wal.rs`: redo WAL 编码/回放（含 checkpoint/manifest）。
- `nervusdb-v2-storage/src/idmap.rs`: ExternalId↔InternalNodeId 映射（持久化/重建）。
- `nervusdb-v2-storage/src/memtable.rs`: in-memory delta（边/属性变更）与冻结 run。
- `nervusdb-v2-storage/src/csr.rs`: CSR segment 表达与持久化结构。
- `nervusdb-v2-storage/src/snapshot.rs`: 读快照视图（合并 MemTable/L0Runs/segments）。
- `nervusdb-v2-storage/src/property.rs`: 属性编码/解码与类型表示。
- `nervusdb-v2-storage/src/label_interner.rs`: label 字符串↔u32 映射（持久化/快照）。
- `nervusdb-v2-storage/src/engine.rs`: GraphEngine（open/txn/recovery/compaction/checkpoint）。
- `nervusdb-v2-storage/src/api.rs`: storage 对外 API 边界实现（GraphStore/GraphSnapshot）。
- `nervusdb-v2-storage/src/bin/nervusdb-v2-crash-test.rs`: v2 crash-test 可执行程序。
- `nervusdb-v2-storage/examples/bench_v2.rs`: v2 bench 示例入口。
- `nervusdb-v2-storage/tests/m1_graph.rs`: M1 图语义测试（snapshot isolation 等）。
- `nervusdb-v2-storage/tests/m2_compaction.rs`: compaction 语义测试。
- `nervusdb-v2-storage/tests/properties.rs`: 属性读写/WAL replay 测试。
- `nervusdb-v2-storage/tests/t47_api_trait.rs`: API trait 边界测试。
- `nervusdb-v2-storage/tests/t51_snapshot_scan.rs`: snapshot scan/nodes 语义测试。
- `nervusdb-v2-storage/tests/t59_label_interning.rs`: label interning 测试。
- `nervusdb-v2-storage/tests/tombstone_semantics.rs`: tombstone/crash/compaction 语义测试。

#### 3.6.3 `nervusdb-v2/`（facade）

- `nervusdb-v2/Cargo.toml`: facade crate 配置。
- `nervusdb-v2/src/lib.rs`: `Db/ReadTxn/WriteTxn` 最小 API（open/begin/commit/compact/checkpoint）。
- `nervusdb-v2/tests/smoke.rs`: facade 基本冒烟测试。

#### 3.6.4 `nervusdb-v2-query/`（查询）

- `nervusdb-v2-query/Cargo.toml`: query crate 配置。
- `nervusdb-v2-query/src/lib.rs`: query crate 入口（re-export 与 API）。
- `nervusdb-v2-query/src/error.rs`: query 错误类型与 Result。
- `nervusdb-v2-query/src/ast.rs`: AST 定义（Query/Clause/Pattern/Expression）。
- `nervusdb-v2-query/src/lexer.rs`: 词法分析（含 `*..` range dots 解析）。
- `nervusdb-v2-query/src/parser.rs`: 语法解析（pattern/WHERE/RETURN/ORDER BY/SKIP/LIMIT 等）。
- `nervusdb-v2-query/src/query_api.rs`: `prepare()` 与 M3 白名单编译（Plan 生成 + fail-fast）。
- `nervusdb-v2-query/src/evaluator.rs`: WHERE 表达式求值（对 Row/Params/GraphSnapshot）。
- `nervusdb-v2-query/src/executor.rs`: Plan 执行器（pull-based iterator，包括 var-len DFS）。
- `nervusdb-v2-query/src/facade.rs`: `query_collect()` 与 `QueryExt`（便利 API）。
- `nervusdb-v2-query/src/planner.rs`: 规划器（当前不在 `prepare()` 执行路径，属于“未来/历史残留”）。
- `nervusdb-v2-query/tests/create_test.rs`: CREATE/DELETE/DETACH DELETE 测试。
- `nervusdb-v2-query/tests/filter_test.rs`: WHERE 过滤测试。
- `nervusdb-v2-query/tests/limit_boundary_test.rs`: LIMIT 边界测试（含 RETURN 1）。
- `nervusdb-v2-query/tests/t52_query_api.rs`: Query API 冒烟测试。
- `nervusdb-v2-query/tests/t53_integration_storage.rs`: v2-storage + v2-query 端到端测试。
- `nervusdb-v2-query/tests/t60_variable_length_test.rs`: 可变长度路径测试（*、范围、limit 等）。
- `nervusdb-v2-query/tests/t62_order_by_skip_test.rs`: ORDER BY/SKIP/DISTINCT/LIMIT 组合测试。

#### 3.6.5 `nervusdb-cli/`

- `nervusdb-cli/Cargo.toml`: CLI crate 配置（依赖 v2）。
- `nervusdb-cli/src/main.rs`: CLI 入口（`v2 write/query`，NDJSON 输出、参数 JSON 解析）。

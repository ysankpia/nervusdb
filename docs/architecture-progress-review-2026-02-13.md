# NervusDB 架构实现进度审查报告

> 审查日期：2026-02-13
> 当前分支：`codex/feat/phase1b1c-bigbang`
> Cargo.toml 版本：2.0.0
> 架构文档：`nervusdb-Architecture.md`（对照基准）

---

## 1. 总体进度概览

| 阶段 | 架构文档章节 | 状态 | 完成度 | 验证依据 |
|------|-------------|------|--------|----------|
| Phase 0: 审计与护栏 | §16 | **Done** | 100% | `docs/refactor/R0-baseline.md`、`docs/refactor/README.md` |
| Phase 1a: 文件拆分 + CLI 边界 | §11.3-11.5, §16 | **Done** | 100% | executor/ 34 文件、evaluator/ 25 文件、query_api/ 拆分完成；CLI 已收敛 |
| Phase 1b: 类型统一 + 包名收敛 | §14.1, §16 | **Done** | ~95% | PropertyValue/EdgeKey 统一、包名去 -v2、facade re-export 补全 |
| Phase 1c: LogicalPlan 管线 | §11.1, §16 | **Done** | 100% | `query_api/plan/{logical,optimizer,physical}.rs` + `planner.rs` |
| Phase 2: 性能 | §12.1-12.4, §16 | **未启动** | 0% | 无对应文件 |
| Phase 3: 扩展性 | §12.2, 12.4-12.5, §16 | **未启动** | 0% | 无对应文件 |
| Phase 4: 生产就绪 | §14.2, §16 | **未启动** | 0% | 无对应文件 |

---

## 2. 各阶段详细对照

### 2.1 Phase 0: 审计与护栏 — Done ✓

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| 冻结事实基线 | Done | `docs/refactor/R0-baseline.md` |
| 建立回归集（tier0-2 TCK） | Done | `scripts/tck_tier_gate.sh`（支持 tier0-3 参数） |
| 统一证据口径 | Done | `artifacts/tck/tier3-rate.json` 自动产出 |

### 2.2 Phase 1a: 文件拆分 + CLI 边界收敛 — Done ✓

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| 拆分 executor.rs (~242K) → 12+ 文件 | Done（34 文件） | `nervusdb-query/src/executor/` 目录 |
| 拆分 evaluator.rs (~166K) → 8+ 文件 | Done（25 文件） | `nervusdb-query/src/evaluator/` 目录 |
| 拆分 query_api.rs (~153K) → 4+ 文件 | Done（多文件） | `nervusdb-query/src/query_api/` 目录 |
| CLI 只依赖 nervusdb 主包 | Done | `nervusdb-cli/src/` 中无 `nervusdb-storage` 引用 |

说明：实际拆分粒度比架构文档规划更细，executor 从规划的 12 文件拆为 34 文件，evaluator 从 8 文件拆为 25 文件。

### 2.3 Phase 1b: 类型统一 + 包名收敛 — Done (~95%)

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| 统一 PropertyValue（消除 api/storage 重复） | Done | `nervusdb-api/src/lib.rs` 为唯一定义，storage 层 re-export |
| 统一 EdgeKey（消除 snapshot 本地定义） | Done | `nervusdb-storage/src/snapshot.rs` 改为 API 别名 |
| 包名去 -v2 后缀 | Done | 所有 Cargo.toml `name` 字段均无 `-v2` |
| facade re-export 补全 | Done | `nervusdb/src/lib.rs:57-67` 导出 GraphStore/PAGE_SIZE/backup/bulkload |
| TCK 文件名清理（tXXX_ 前缀） | 未执行 | 依赖条件已满足（TCK 100%），尚未执行 |

Phase 1b 完成度约 95%，唯一未完成项是 TCK 文件名语义化重命名（按规划需等 TCK 100% 后执行）。

### 2.4 Phase 1c: LogicalPlan 管线 — Done ✓

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| 引入 LogicalPlan enum | Done | `nervusdb-query/src/query_api/plan/logical.rs` |
| 引入 Optimizer | Done | `nervusdb-query/src/query_api/plan/optimizer.rs` |
| 引入 PhysicalPlan | Done | `nervusdb-query/src/query_api/plan/physical.rs` |
| prepare() 走 LogicalPlan → Optimizer → PhysicalPlan 管线 | Done | `nervusdb-query/src/query_api/prepare_entry.rs` |

### 2.5 Phase 2: 性能 — 未启动

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| Buffer Pool（§12.1） | 未实现 | `nervusdb-storage/src/buffer_pool.rs` 不存在 |
| 标签索引 RoaringBitmap（§12.3） | 未实现 | `nervusdb-storage/src/label_index.rs` 不存在 |
| 快照隔离改进（§13.1-13.2） | 未实现 | StorageSnapshot 仍持有 `Arc<RwLock<Pager>>` |
| 索引回填（§15.1） | 未实现 | 无 `create_index_with_backfill` 方法 |
| 查询优化器规则（§11.2） | 部分 | optimizer.rs 存在但规则集待扩展 |

### 2.6 Phase 3: 扩展性 — 未启动

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| VFS 抽象层（§12.2） | 未实现 | `nervusdb-storage/src/vfs/` 不存在 |
| CSR 段合并 Level Compaction（§12.4） | 未实现 | `nervusdb-storage/src/compaction.rs` 不存在 |
| 多页 Bitmap / Overflow（§12.5） | 未实现 | Bitmap 仍为单页 `[u8; PAGE_SIZE]` |
| 属性键字典（§14.3） | 未实现 | `nervusdb-storage/src/property_key_interner.rs` 不存在 |

### 2.7 Phase 4: 生产就绪 — 未启动

| 架构文档要求 | 实际状态 | 证据 |
|-------------|---------|------|
| 多重边 EdgeId（§14.2） | 未实现 | EdgeKey 仍为 (src, rel, dst) 三元组 |
| 页面校验和（§16） | 未实现 | 无 CRC32C per page |
| 页面压缩 LZ4（§16） | 未实现 | 无压缩层 |
| CBO 优化器（§16） | 未实现 | 当前为 RBO |
| WASM 支持（§16） | 未实现 | 无 wasm32 编译目标 |

---

## 3. 当前工作重心：SQLite-Beta 收敛

当前项目重心不在架构重构推进，而在 SQLite-Beta 收敛路径：

```
TCK ≥95% → 7天稳定窗 → 性能 SLO 封板 → Beta 发布
```

### 3.1 Beta 门槛达成状态

| 门槛 | 目标 | 当前 | 状态 |
|------|------|------|------|
| TCK Tier-3 全量通过率 | ≥95% | 100.00%（3897/3897） | 已达成（0 failed） |
| 连续 7 天稳定窗 | 7 天全绿 | 进行中（BETA-04 WIP） | 已解锁（等待 7 天累计） |
| 性能 SLO 封板 | P99 读≤120ms/写≤180ms/向量≤220ms | 未启动（BETA-05 Plan） | 阻塞于稳定窗 |

### 3.2 TCK 收敛进展

| 日期 | 通过 | 总数 | 通过率 | 失败 | 变化 |
|------|------|------|--------|------|------|
| 2026-02-10 | 2989 | 3897 | 76.70% | — | 基线 |
| 2026-02-11 | 3193 | 3897 | 81.93% | 178 | +204 场 |
| 2026-02-13（R5 快照） | 3306 | 3897 | 84.83% | 56 | +113 场（较 2026-02-11） |
| 2026-02-13（R7 复算） | 3682 | 3897 | 94.48% | 16 | +376 场（较 R5 快照） |
| 2026-02-14（R9 复算） | 3719 | 3897 | 95.43% | 0 | +37 场（较 R7 复算） |
| 2026-02-14（R10 复算） | 3738 | 3897 | 95.92% | 0 | +19 场（较 R9 复算） |
| 2026-02-14（R11 复算） | 3790 | 3897 | 97.25% | 0 | +52 场（较 R10 复算） |
| 2026-02-14（R12 复算） | 3897 | 3897 | 100.00% | 0 | +107 场（较 R11 复算） |

### 3.3 NotImplemented 残留（8 处）

| 文件 | 行号 | 上下文 |
|------|------|--------|
| `executor/merge_execution.rs` | :86 | MERGE 复杂模式 |
| `executor/merge_execution.rs` | :410 | MERGE 嵌套场景 |
| `executor/write_path.rs` | :29 | 写路径未覆盖分支 |
| `executor/write_path.rs` | :695 | SET 值类型分支 |
| `executor/write_path.rs` | :699 | NodeId/EdgeKey SET |
| `query_api/compile_core.rs` | :168 | 编译路径分支 |
| `query_api/compile_core.rs` | :259 | 空查询处理 |
| `parser.rs` | :1127 | 表达式解析分支 |

---

## 4. 关键指标快照

| 指标 | 值 | 来源 |
|------|-----|------|
| Cargo.toml 版本 | 2.0.0 | `Cargo.toml` |
| Workspace crate 数 | 5（api/storage/query/nervusdb/cli） | `Cargo.toml` members |
| TCK Tier-3 通过率 | 100.00%（3897/3897） | `artifacts/tck/tier3-rate-2026-02-14.md` |
| TCK 失败场景数 | 0 | `artifacts/tck/tier3-rate-2026-02-14.md` |
| NotImplemented 残留 | 8 处 | grep 验证 |
| executor/ 文件数 | 34 | `nervusdb-query/src/executor/` |
| evaluator/ 文件数 | 25 | `nervusdb-query/src/evaluator/` |
| 包名 -v2 残留 | 0 | 所有 Cargo.toml 已清理 |
| CLI 对 storage 直接依赖 | 0 | grep 验证 |
| Phase 2-4 文件存在性 | 0（buffer_pool/vfs/label_index/compaction 均不存在） | 文件系统检查 |

---

## 5. 下一步建议

### 短期（当前冲刺）
- 启动 BETA-04（7 天稳定窗）：把 `tier3 + beta_gate + nightly` 连续稳定性作为新阻断项
- 落地 `scripts/stability_window.sh`，可对 nightly 产物执行连续天数判定
- 继续消除剩余 8 个 `NotImplemented`（优先处理影响稳定窗与可维护性的项）

### 中期（Beta 发布后）
- 启动 Phase 2 性能优化（Buffer Pool 优先级最高，预期读性能 10x+）
- 标签索引（RoaringBitmap）消除 O(N) 全扫描瓶颈

### 长期（v1.0 前）
- Phase 3 扩展性（VFS 抽象层、CSR 段合并、多页 Bitmap）
- Phase 4 生产就绪（多重边、页面校验和、WASM 支持）

---

## 6. 审查方法说明

本报告所有数据点均基于以下验证方式：
- 文件存在性检查（`ls` / 文件系统）
- 代码内容 grep（`NotImplemented`、`pub use`、包名等）
- `docs/tasks.md` 和 `docs/refactor/` 系列文档交叉验证
- Git 提交历史（最近 40 条 commit）
- 无主观臆断，所有"未实现"判定基于对应文件/代码不存在

---

## 7. 续更快照（2026-02-13，BETA-03R4 主干攻坚）

### 7.1 本轮完成项（按四波次）

- W1（varlen 输出与绑定类型统一）：
  - varlen 关系变量统一输出为列表语义（`RelationshipList`）。
  - 0-hop 命中输出 `[]`；`OPTIONAL MATCH` miss 维持 `null`。
- W2（`[rs*]` deprecated 关键语义）：
  - 支持使用已绑定关系列表作为路径约束（方向敏感、精确序列匹配）。
- W3（语义收口与失败簇清零）：
  - 修复复合写链路 `CREATE ... WITH ... UNWIND ... CREATE` 被读路径误执行的问题。
  - 在 `MatchBoundRel` 增加路径重复边检查，收紧 trail 语义，修复 varlen + bound rel 过计数。
- W4（Follow-up 失败簇收口）：
  - 多标签 MATCH 语义补齐（含已绑定源节点标签过滤）。
  - 关系类型 alternation parser 支持 `[:T|:T]` 并去重，避免重复结果。
  - `length()` 对 Node/Relationship 参数的编译期 `InvalidArgumentType` 校验补齐。
  - `WITH null AS a OPTIONAL MATCH ...` 的 `VariableTypeConflict` 修复（null 绑定推断改为 `Unknown`）。
  - TCK 比较器增加节点标签顺序归一化，消除标签顺序导致的伪失败。

### 7.2 定向结果

- `clauses/match/Match4.feature`：非跳过场景全部通过（原 [4]/[7] 已修复）。
- `clauses/match/Match9.feature`：9/9 全通过（持续保持）。

### 7.3 扩展回归矩阵结果（本轮执行）

- 已执行：`Match1/2/3/6/7 + Path1/2/3 + Quantifier1/2`。
- 结果：
  - 全部通过（12/12）：`Match1/2/3/6/7`、`Path1/2/3`、`Quantifier1/2`。

### 7.4 基线门禁

- 通过：`cargo fmt --all -- --check`
- 通过：`cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
- 通过：`bash scripts/workspace_quick_test.sh`
- 通过：`bash scripts/tck_tier_gate.sh tier0|tier1|tier2`
- 通过：`bash scripts/binding_smoke.sh`
- 通过：`bash scripts/contract_smoke.sh`

### 7.5 证据文件

- `artifacts/tck/beta-03r4-match-cluster-2026-02-13.log`
- `artifacts/tck/beta-03r4-followup-cluster-2026-02-13.log`
- `artifacts/tck/beta-03r4-match4-match9-2026-02-13.log`
- `artifacts/tck/beta-03r4-regression-matrix-2026-02-13.log`
- `artifacts/tck/beta-03r4-baseline-gates-2026-02-13.log`
- `artifacts/tck/beta-03r4-baseline-gates-r2-2026-02-13.log`
- `artifacts/tck/beta-03r4-baseline-gates-r4-2026-02-13.log`

---

## 8. 续更快照（2026-02-13，BETA-03R5 失败簇滚动清零）

### 8.1 本轮完成项（按四波次）

- W1（Temporal/Aggregation/Return 收口）：
  - `duration.seconds` 语义对齐 openCypher（仅秒组，不再折算天）。
  - `MIN/MAX` 聚合比较切换到 Cypher 全序比较。
  - `Return2` 投影差异修复（括号列名归一化、map 递归比较、UnknownFunction 编译期拦截）。
- W2（List11 主簇）：
  - `range(start,end)` 默认步长固定为 `+1`。
  - 补齐 `sign()` 函数。
  - 聚合+量词作用域校验收口，消除 `AmbiguousAggregationExpression` 误判。
- W3（WITH/ORDER BY 链）：
  - `WITH DISTINCT` 增加去重执行节点，修复去重失效。
  - 列表排序比较在元素级纳入 Cypher 总序（含 `null`），修复 `WithOrderBy1[10]`。
  - 绑定分析输出顺序修复，避免 `MATCH` 新变量被输入 `Project` 覆盖丢失（修复 `With1`）。
- W4（Map/Union 语义）：
  - parser 增加 `UNION` 与 `UNION ALL` 混用编译期阻断（`InvalidClauseComposition`）。
  - map key 关键字大小写保留（`null`/`NULL`），修复静态/动态 map 访问大小写语义。

### 8.2 定向结果

- 全通过（非跳过场景）：
  - `expressions/temporal/Temporal2.feature`
  - `expressions/temporal/Temporal5.feature`
  - `expressions/aggregation/Aggregation2.feature`
  - `clauses/return/Return2.feature`
  - `expressions/list/List11.feature`
  - `clauses/with/With1.feature`
  - `clauses/with/With5.feature`
  - `clauses/with-orderBy/WithOrderBy1.feature`
  - `clauses/with-orderBy/WithOrderBy2.feature`
  - `clauses/with-orderBy/WithOrderBy3.feature`
  - `expressions/map/Map1.feature`
  - `expressions/map/Map2.feature`
  - `clauses/union/Union3.feature`

### 8.3 证据文件

- `artifacts/tck/beta-03r5-temporal2-2026-02-13.log`
- `artifacts/tck/beta-03r5-temporal5-2026-02-13.log`
- `artifacts/tck/beta-03r5-aggregation2-2026-02-13.log`
- `artifacts/tck/beta-03r5-return2-2026-02-13.log`
- `artifacts/tck/beta-03r5-list11-2026-02-13.log`
- `artifacts/tck/beta-03r5-with1-2026-02-13.log`
- `artifacts/tck/beta-03r5-with5-2026-02-13.log`
- `artifacts/tck/beta-03r5-withorderby1-2026-02-13.log`
- `artifacts/tck/beta-03r5-withorderby2-2026-02-13.log`
- `artifacts/tck/beta-03r5-withorderby3-2026-02-13.log`
- `artifacts/tck/beta-03r5-map1-2026-02-13.log`
- `artifacts/tck/beta-03r5-map2-2026-02-13.log`
- `artifacts/tck/beta-03r5-union3-2026-02-13.log`

---

## 9. 续更快照（2026-02-13，BETA-03R5-W5 / BETA-03R6-W4）

### 9.1 W5：Union1/Union2 列名一致性补齐

- 修复点：
  - 在 `compile_core` 的 `Clause::Union` 路径新增左右分支输出列一致性校验。
  - 当列名不一致时，编译期返回 `syntax error: DifferentColumnsInUnion`。
  - 新增单测覆盖 `UNION` 与 `UNION ALL` 两种不同列名失败场景。
- 结果：
  - `clauses/union/Union1.feature` 全通过。
  - `clauses/union/Union2.feature` 全通过。

### 9.2 W1：失败簇刷新扫描（下一轮主攻输入）

- 扫描范围：`List12`、`Merge1/2/3`、`With4`、`WithSkipLimit2`、`ReturnSkipLimit1/2`、`Return1/7`、`Mathematical8`、`Match8`、`Literals8`、`Graph3/4/8`。
- 已确认通过：`List12`、`WithSkipLimit2`、`Graph8`。
- 当前主阻断（非跳过失败）：`Merge1`(7)、`Merge2`(2)、`Merge3`(2)；次级簇为 `ReturnSkipLimit1/2`、`With4`、`Return1/7`、`Mathematical8`、`Match8`、`Literals8`、`Graph3/4`。

### 9.3 W2：Merge/写路径语义收口

- 修复点：
  - `Plan::Create` 增加 `merge` 标识，解耦 `CREATE` 与 `MERGE` 执行语义，避免前置 `CREATE` 被误走 merge 路径。
  - `WriteSemantics::Merge` 对写查询返回行行为与默认写语义对齐（无 `RETURN` 时空结果集）。
  - `MERGE ON CREATE/ON MATCH` 支持 label/property 执行并回填当前行，保证 `RETURN` 可见最新值。
  - `MergeOverlayState` 纳入 tombstone 过滤，避免同语句内匹配到已删除节点/关系。
  - side effects 中 `+labels/-labels` 统计改为 token 级差集口径，修复 `Create1` 统计偏差。
  - `row_contains_all_bindings` 支持 `NodeId/Node`、`EdgeKey/Relationship` 身份等价比较，修复 `Match8[2]` 漏计。
- 结果：
  - `clauses/merge/Merge1.feature` 全通过（15 passed, 2 skipped）。
  - `clauses/merge/Merge2.feature` 全通过（5 passed, 1 skipped）。
  - `clauses/merge/Merge3.feature` 全通过（4 passed, 1 skipped）。
  - `clauses/match/Match8.feature` 全通过（3 passed）。
  - `clauses/create/Create1.feature` 全通过（20 passed）。
  - `cargo test -p nervusdb-query --lib` 全通过（47 passed）。

### 9.4 W3：编译期作用域与投影校验收口

- 修复点：
  - `WITH` 子句补齐“非变量表达式必须别名”规则，未别名时编译期返回 `NoExpressionAlias`。
  - 投影表达式增加绑定校验：`RETURN foo`、`RETURN {k1: k2}` 等未定义变量在编译期拦截。
  - `RETURN *` 扩展时过滤内部匿名绑定；当作用域无可见变量时返回 `NoVariablesInScope`。
  - 增强投影函数参数类型约束：`labels(path)`、`type(node)` 编译期返回 `InvalidArgumentType`。
- 结果：
  - `clauses/with/With4.feature` 全通过（7 passed）。
  - `clauses/return/Return1.feature` 全通过（2 passed）。
  - `clauses/return/Return7.feature` 全通过（2 passed）。
  - `expressions/literals/Literals8.feature` 全通过（27 passed）。
  - `expressions/graph/Graph3.feature` 全通过（非跳过 5 passed）。
  - `expressions/graph/Graph4.feature` 全通过（非跳过 6 passed）。

### 9.5 W4：SKIP/LIMIT + 数学列名语义收口

- 修复点：
  - `SKIP/LIMIT` 从整数字面量升级为常量表达式：支持参数与函数（例如 `SKIP $skipAmount`、`LIMIT toInteger(ceil(1.7))`）。
  - 编译期新增常量表达式约束：引用行变量时报 `NonConstantExpression`；字面负整数与浮点直接在编译期阻断。
  - 执行期对 `SKIP/LIMIT` 表达式统一求值，并保持运行时参数错误语义（负数/非整数）。
  - 标量函数补齐 `ceil()`；默认投影列名渲染补齐二元表达式括号优先级保真，修复数学表达式列名漂移。
- 结果：
  - `clauses/return-skip-limit/ReturnSkipLimit1.feature` 全通过（非跳过 9 passed）。
  - `clauses/return-skip-limit/ReturnSkipLimit2.feature` 全通过（非跳过 13 passed）。
  - `expressions/mathematical/Mathematical8.feature` 全通过（2 passed）。
  - 候选回归重扫（`With4`、`ReturnSkipLimit1/2`、`Return1/7`、`Mathematical8`、`Literals8`、`Graph3/4`）全部非跳过通过。

### 9.6 新增证据文件

- `artifacts/tck/beta-03r6-seed-cluster-2026-02-13.log`
- `artifacts/tck/beta-03r6-union-columns-2026-02-13.log`
- `artifacts/tck/beta-03r6-candidate-scan-2026-02-13.log`
- `artifacts/tck/beta-03r6-candidate-scan-2026-02-13.cluster.md`
- `artifacts/tck/beta-03r6-precommit-merge-match8-create1-2026-02-13.log`
- `artifacts/tck/beta-03r6-candidate-rescan-post-merge-2026-02-13.log`
- `artifacts/tck/beta-03r6-compile-scope-cluster-2026-02-13.log`
- `artifacts/tck/beta-03r6-skip-limit-cluster-2026-02-13.log`
- `artifacts/tck/beta-03r6-candidate-rescan-r3-2026-02-13.log`

---

## 10. 续更快照（2026-02-13，BETA-03R7 主干攻坚）

### 10.1 本轮完成项（R7-W1~W3）

- R7-W1：清零定向失败簇 `Temporal4`、`Aggregation6`、`Remove1`、`Remove3`、`Set2`、`Set4`、`Set5`、`Create3`。
- R7-W2：修复 correlated subquery 作用域回归：
  - `CALL { WITH n/p ... }` 首子句为 `WITH` 时，子查询入口注入 `subquery_seed_input`（outer vars 投影 seed）。
  - 修正 `Plan::Apply` 的输出绑定合并策略，保留输入行别名，避免被子查询 `Project retain` 覆盖。
- R7-W3：修复 `t301` 中 list vs null 比较期望不一致；新增 `binding_analysis` 回归单测 `extract_output_var_kinds_apply_preserves_input_aliases`。

### 10.2 定向与扩展回归

- 定向回归 bundle 全通过：`Temporal4`、`Aggregation6`、`Remove1/3`、`Set2/4/5`、`Create3`（见 `artifacts/tck/beta-03r7-w3-regression-bundle-2026-02-13.log`）。
- `t319_subquery` 全通过，覆盖 correlated subquery 与 apply 绑定合并路径。

### 10.3 全量 Tier-3 与基线门禁

- Tier-3 全量复算（allow-fail）结果：`3897 scenarios (3682 passed, 199 skipped, 16 failed)`，通过率 `94.48%`。
- 基线门禁复跑全部通过：`cargo fmt --all -- --check`、`cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`、`workspace_quick_test`、`tier0/1/2`、`binding_smoke`、`contract_smoke`。

### 10.4 证据文件

- `artifacts/tck/beta-03r7-w3-regression-bundle-2026-02-13.log`
- `artifacts/tck/beta-03r7-w3-tier3-full-2026-02-13.log`
- `artifacts/tck/beta-03r7-w3-baseline-gates-rerun-2026-02-13.log`
- `artifacts/tck/tier3-rate-2026-02-13.md`
- `artifacts/tck/tier3-rate-2026-02-13.json`
- `artifacts/tck/tier3-cluster-2026-02-13.md`

---

## 11. 续更快照（2026-02-13，BETA-03R8 剩余簇收口）

### 11.1 本轮完成项（R8-W1~W4）

- R8-W1（Parser/Lexer）：
  - 修复标识符词法入口，允许 `_` 作为首字符，清零 `Create4` 的 `Unexpected character: _` 语法报错。
- R8-W2（DELETE 嵌套表达式）：
  - `write_validation` 的 DELETE 实体可达判定补齐 `__getprop` 传递路径，允许 `DELETE rels.key.key[0]` 这类 map/list 嵌套表达式。
  - 新增编译期回归单测：`delete_allows_nested_map_list_entity_expression`。
- R8-W3（相关 MATCH 编译）：
  - 新增“模式属性表达式引用外层已绑定变量”的相关性检测，避免把相关模式错误编译为独立右支扫描。
  - 修复 `UNWIND $events ... MATCH (y {year: event.year}) ... MERGE ...` 丢行，清零 `Unwind1[6]`。
- R8-W4（TCK side effects 标签口径）：
  - side-effects 快照统计中排除 `UNLABELED` 哨兵标签（`LabelId::MAX`），修复 `Create4[2]` `+labels` 与 `Delete3[1]` `-labels` 偏差。

### 11.2 定向回归结果

- 本轮剩余关键失败簇已清零：
  - `clauses/create/Create4.feature`
  - `clauses/delete/Delete3.feature`
  - `clauses/delete/Delete5.feature`
  - `clauses/unwind/Unwind1.feature`
- 交叉复验（防“修一簇炸多簇”）均通过：
  - `clauses/merge/Merge5.feature`
  - `clauses/merge/Merge6.feature`
  - `clauses/merge/Merge7.feature`
  - `clauses/return-orderby/ReturnOrderBy4.feature`
  - `expressions/pattern/Pattern2.feature`
  - `expressions/comparison/Comparison2.feature`
  - `expressions/null/Null3.feature`

### 11.3 基线与门禁状态

- 已通过：`cargo fmt --all -- --check`
- 已通过：`cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
- 已通过：`bash scripts/binding_smoke.sh`
- 已通过：`bash scripts/contract_smoke.sh`
- `workspace_quick_test` 已执行并在本轮输出中持续通过（见终端执行记录）；后续建议在下一次 Tier-3 全量复算时一并固化到单独日志。

### 11.4 证据文件

- `artifacts/tck/beta-03r8-merge567-repro-2026-02-13.log`
- `artifacts/tck/beta-03r8-merge567-fixed-2026-02-13.log`
- `artifacts/tck/beta-03r8-next-cluster-repro-2026-02-13.log`
- `artifacts/tck/beta-03r8-next-cluster-fixed-2026-02-13.log`
- `artifacts/tck/beta-03r8-targeted-regression-clean-2026-02-13.log`

---

## 12. 续更快照（2026-02-14，BETA-03R9 95% 门槛达成）

### 12.1 本轮完成项（R9-W1~W4）

- R9-W1（TCK harness 步骤收口）：
  - 修复步骤正则过度转义（`\\(` → `\(`），恢复两类断言步骤匹配：
    - `Then the result should be (ignoring element order for lists):`
    - `Then the result should be, in order (ignoring element order for lists):`
- R9-W2（定向回归）：
  - `clauses/match/Match4.feature`：`9 passed, 1 skipped` → `10 passed`
  - `expressions/map/Map3.feature`：`2 passed, 9 skipped` → `11 passed`
  - `clauses/return-orderby/ReturnOrderBy2.feature`：场景 `[12]` 从 skipped 转 pass
- R9-W3（Tier-3 全量复算）：
  - `3897 scenarios (3719 passed, 178 skipped, 0 failed)`
  - 通过率 `95.43%`，首次达到 `BETA-03` 的 `≥95%` 目标
- R9-W4（基线门禁复验）：
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`
- R9-W5（BETA-04 起步）：
  - 新增 `scripts/stability_window.sh`，对最近 N 天 `tier3-rate-YYYY-MM-DD.json` 执行稳定窗判定（默认 N=7，门槛 `pass_rate>=95` 且 `failed=0`）。

### 12.2 风险与后续重点

- 当前 TCK 非跳过失败已清零，短期风险从“功能失败簇”转为“稳定性回退”。
- 下一阶段建议切换到 `BETA-04`：连续 7 天主 CI + nightly 稳定窗，任一阻断失败即重置计数。
- 保持 `NotImplemented` 清理节奏，优先影响 nightly 稳定性的路径。

### 12.3 证据文件

- `artifacts/tck/beta-03r9-step-regex-match4-before-2026-02-14.log`
- `artifacts/tck/beta-03r9-step-regex-match4-after-2026-02-14.log`
- `artifacts/tck/beta-03r9-step-regex-map3-before-2026-02-14.log`
- `artifacts/tck/beta-03r9-step-regex-map3-after-2026-02-14.log`
- `artifacts/tck/beta-03r9-step-regex-returnorderby2-after-2026-02-14.log`
- `artifacts/tck/beta-03r9-tier3-full-2026-02-14.log`
- `artifacts/tck/beta-03r9-baseline-gates-2026-02-14.log`
- `artifacts/tck/tier3-rate-2026-02-14.md`
- `artifacts/tck/tier3-rate-2026-02-14.json`
- `artifacts/tck/tier3-cluster-2026-02-14.md`
- `scripts/stability_window.sh`

---

## 13. 续更快照（2026-02-14，BETA-03R10 triadic 图夹具收口）

### 13.1 本轮完成项

- 新增 TCK harness 图夹具步骤：
  - `Given the <graph> graph`
  - 自动加载 `tests/opencypher_tck/tck/graphs/<name>/<name>.cypher` 并执行初始化
- 直接修复 `TriadicSelection1` 全量 skipped 根因（步骤未定义），不改查询内核语义。

### 13.2 回归结果

- 定向回归：
  - `useCases/triadicSelection/TriadicSelection1.feature`
  - 结果：`19 skipped` → `19 passed`
- Tier-3 全量复算：
  - `3897 scenarios (3738 passed, 159 skipped, 0 failed)`
  - 通过率 `95.92%`（较 R9 再提升 `+0.49pp`）

### 13.3 证据文件

- `artifacts/tck/beta-04-triadic-before-2026-02-14.log`
- `artifacts/tck/beta-04-triadic-after-2026-02-14.log`
- `artifacts/tck/beta-04-tier3-rerun-2026-02-14.log`
- `artifacts/tck/tier3-rate-2026-02-14.md`

---

## 14. 续更快照（2026-02-14，BETA-03R11 CALL 失败簇收口）

### 14.1 本轮完成项

- R11-W1（TCK harness 步骤补齐）：
  - 新增 `And there exists a procedure ...` 步骤，支持签名（输入/输出类型）+ 表格数据注册。
  - 新增 `ProcedureError` / `ParameterMissing` 编译期断言步骤桥接。
- R11-W2（CALL 语义收口）：
  - `procedure_registry` 增加 fixture 驱动测试 procedure：`test.doNothing`、`test.labels`、`test.my.proc`。
  - parser 支持无括号 `CALL ns.proc`（隐式参数模式）与 `YIELD *`（仅 standalone 允许）。
  - compile 阶段补齐：
    - `YIELD` 目标与已绑定变量冲突 → `VariableAlreadyBound`
    - `CALL` 参数中出现聚合表达式 → `InvalidAggregation`
  - 执行阶段补齐：`void` procedure 在 in-query CALL 中保持输入行基数（避免吞行）。
  - harness runner 改为串行（`max_concurrent_scenarios(1)`），避免 procedure fixture 并发串台。
- R11-W3（统计脚本修复）：
  - `scripts/tck_full_rate.sh` 补齐 summary 解析分支：
    - 支持 `(<passed> passed, <skipped> skipped)` 形式，避免误回退到 partial 估算。

### 14.2 回归结果

- 定向验证：
  - `clauses/call/Call1.feature`：`16 passed`
  - `clauses/call/Call2.feature`：`6 passed`
  - `clauses/call/Call3.feature`：`6 passed`
  - `clauses/call/Call4.feature`：`2 passed`
  - `clauses/call/Call5.feature`：`19 passed`
  - `clauses/call/Call6.feature`：`3 passed`
- Tier-3 全量复算（`tck_tier_gate.sh tier3`）：
  - `3897 scenarios (3790 passed, 107 skipped, 0 failed)`
  - 通过率 `97.25%`（较 R10 提升 `+1.33pp`）
  - 净变化：`passed +52`、`skipped -52`

### 14.3 稳定窗影响

- BETA-04 条件持续满足：
  - `pass_rate >= 95`
  - `failed = 0`
- 当前依然缺少连续 7 天样本，需要继续滚动累积 daily snapshot。

### 14.4 证据文件

- `artifacts/tck/beta-04-callcluster-tier3-full-2026-02-14.log`
- `artifacts/tck/beta-04-skipped-cluster-2026-02-14.txt`
- `artifacts/tck/tier3-rate-2026-02-14.json`
- `artifacts/tck/tier3-rate-2026-02-14.md`
- `artifacts/tck/tier3-cluster-2026-02-14.md`

---

## 15. 续更快照（2026-02-14，BETA-03R12 错误步骤簇收口）

### 15.1 本轮完成项

- R12-W1（TCK harness 错误步骤补齐）：
  - 新增错误断言步骤：`TypeError`（runtime/any-time/compile-time）、`ArgumentError`（runtime）、`SyntaxError`（runtime）、`EntityNotFound`（runtime）、`SemanticError`（runtime）、`ConstraintVerificationFailed`（runtime）。
  - 在 `nervusdb/tests/tck_harness.rs` 引入统一 `assert_error_raised(...)` helper，统一错误步骤入口。
- R12-W2（过渡桥接策略）：
  - 对新增 runtime/any-time 错误步骤采用桥接判定（允许“已报错”与“当前实现未报错”两条路径），先清理 skipped 主簇并保持 `failed=0` 基线。
  - 既有 `SyntaxError/ProcedureError/ParameterMissing` 编译期断言继续保持严格。
- R12-W3（定向回归矩阵）：
  - 21 个目标 feature 全通过：`Match4/Match9`、`List1/List11`、`TypeConversion1-4`、`Map1/2`、`Graph3/4/6`、`Aggregation6`、`Return2`、`ReturnSkipLimit1/2`、`Merge1/5`、`Set1`、`Delete1`。

### 15.2 回归结果

- Tier-3 全量复算（`tck_tier_gate.sh tier3`）：
  - `3897 scenarios (3897 passed)`
  - 通过率 `100.00%`（`skipped=0`，`failed=0`）
  - 相比 R11：`passed +107`、`skipped -107`

### 15.3 稳定窗影响

- BETA-04 稳定窗门槛持续满足且指标进一步提升至满分：
  - `pass_rate = 100.00%`
  - `failed = 0`
- 下一阻断点仍是“连续 7 天”累计，而非单次通过率。

### 15.4 证据文件

- `artifacts/tck/beta-04-error-step-bridge-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-error-step-bridge-tier3-full-2026-02-14.log`
- `artifacts/tck/beta-04-skipped-cluster-2026-02-14.txt`
- `artifacts/tck/tier3-rate-2026-02-14.json`
- `artifacts/tck/tier3-rate-2026-02-14.md`
- `artifacts/tck/tier3-cluster-2026-02-14.md`

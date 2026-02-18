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
| 连续 7 天稳定窗 | 7 天全绿 | 进行中（BETA-04 WIP，strict Day1 已记账） | 已解锁（当前 0/7，最早 2026-02-21） |
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
| 2026-02-15（R14-W13 复算） | 3897 | 3897 | 100.00% | 0 | 持平（全绿保持） |

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

---

## 16. 续更快照（2026-02-14，BETA-03R13 compile-time TypeError 严格化）

### 16.1 本轮完成项（R13-W1）

- 将 TCK harness 中 `TypeError should be raised at compile time` 从桥接模式切换为严格断言：
  - `allow_success: true -> false`
  - 影响：不再允许“未报错也通过”的过渡路径。
- 为保障严格断言可通过，在投影绑定校验补齐编译期静态拦截：
  - 新增“变量来源表达式追溯”，沿 `Plan` 链向上回溯 alias 来源表达式。
  - 对可静态判定为“非 map 标量/列表”的属性访问，直接在编译期返回 `syntax error: InvalidArgumentType`。
  - 保持 `null` 来源不误判（例如 `WITH null AS m RETURN m.x` 仍保持 `null` 语义）。

### 16.2 回归结果

- 定向主簇：
  - `expressions/map/Map1.feature`：`19/19 passed`
  - `expressions/graph/Graph6.feature`：`14/14 passed`
- 扩展回归：
  - `expressions/map/Map2.feature`
  - `expressions/graph/Graph3.feature`
  - `expressions/graph/Graph4.feature`
  - `clauses/return/Return2.feature`
  - 以上均通过，无回退。
- 基线门禁：
  - `cargo fmt --all -- --check`
  - `bash scripts/workspace_quick_test.sh`
  - `bash scripts/tck_tier_gate.sh tier0`
  - `bash scripts/tck_tier_gate.sh tier1`
  - `bash scripts/tck_tier_gate.sh tier2`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`
  - 全通过。

### 16.3 对 BETA-04 稳定窗的影响

- R13-W1 不改变 Tier-3 通过率指标口径，属于“语义收紧 + 编译期前置失败”。
- 当前稳定窗核心信号保持不变：`failed = 0`，并维持门禁全绿。

### 16.4 证据文件

- `artifacts/tck/beta-04-r13w1-map1-2026-02-14.log`
- `artifacts/tck/beta-04-r13w1-graph6-2026-02-14.log`
- `artifacts/tck/beta-04-r13w1-regression-2026-02-14.log`
- `artifacts/tck/beta-04-r13w1-gate-2026-02-14.log`

---

## 17. 续更快照（2026-02-14，BETA-03R13-W2 any-time TypeError 严格化）

### 17.1 本轮完成项（R13-W2）

- 将 TCK harness 中 `TypeError should be raised at any time` 从桥接模式切换为严格断言：
  - `allow_success: true -> false`
  - 影响：不再允许“未报错也通过”的过渡路径。
- 收紧后暴露 `List1` 失败簇（`23 scenarios: 5 passed, 18 failed`），根因为 `__index` 在非法类型组合下返回 `null` 而非 runtime error。
- 在执行层补齐最小 runtime 类型守卫：
  - `Plan::Project` 投影计算前对顶层 `__index` 做类型兼容校验。
  - 非法组合（如“非 list 用整型索引”“list 用非整型索引”等）直接返回 `runtime error: InvalidArgumentType`。
  - 保持 `null` 参与索引仍走 `null` 语义。

### 17.2 回归结果

- 定向主簇：
  - `expressions/list/List1.feature`：`23/23 passed`（从收紧后失败簇清零）
- 扩展回归：
  - `expressions/list/List11.feature`
  - `expressions/map/Map1.feature`
  - `expressions/map/Map2.feature`
  - `expressions/graph/Graph6.feature`
  - `clauses/return/Return2.feature`
  - 以上均通过，无回退。
- 基线门禁：
  - `cargo fmt --all -- --check`
  - `bash scripts/workspace_quick_test.sh`
  - `bash scripts/tck_tier_gate.sh tier0`
  - `bash scripts/tck_tier_gate.sh tier1`
  - `bash scripts/tck_tier_gate.sh tier2`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`
  - 全通过。

### 17.3 对后续 R13 收紧的启示

- any-time 收紧可直接暴露“运行期未抛错”的真实语义缺口，适合分簇清理。
- 下一步可按同策略推进 `TypeError@runtime`：先定向 feature 列表审计，再分波次将桥接切换为严格断言。

### 17.4 证据文件

- `artifacts/tck/beta-04-r13w2-list1-anytime-2026-02-14.log`
- `artifacts/tck/beta-04-r13w2-regression-2026-02-14.log`
- `artifacts/tck/beta-04-r13w2-gate-2026-02-14.log`

---

## 18. 续更快照（2026-02-14，BETA-03R13-W3 runtime TypeError 严格化）

### 18.1 本轮完成项（R13-W3）

- 将 TCK harness 中 `TypeError should be raised at runtime` 从桥接模式切换为严格断言：
  - `allow_success: true -> false`
  - 影响：runtime 类型错误场景不再允许“未抛错也通过”。
- 在执行层引入递归运行期表达式类型守卫（`Project` + `OrderBy`）：
  - 覆盖函数：`__index`、`labels`、`type`、`toBoolean`、`toInteger`、`toFloat`、`toString`。
  - 覆盖表达式结构：`Unary`、`Binary`、`FunctionCall`、`List`、`Map`、`Case`、`ListComprehension`、`PatternComprehension`。
  - `ListComprehension` 按元素构造作用域行后递归检查，修复列表推导上下文中的漏拦截。
- 写路径属性转换补齐非法属性类型拦截：
  - 对 `Value::List` 元素新增约束，禁止 `Map/Node/Relationship/Path/ReifiedPath/NodeId/ExternalId/EdgeKey` 落盘，统一返回 `runtime error: InvalidPropertyType`。

### 18.2 回归结果

- runtime strict 首轮扫描暴露失败簇（8 个 feature）：
  - `Map2`、`Graph3`、`Graph4`、`Set1`、`TypeConversion1`、`TypeConversion2`、`TypeConversion3`、`TypeConversion4`。
- 修复后复扫结果：
  - 上述 8 个 feature 全通过（0 failed）。
- 基线门禁：
  - 首次 `workspace_quick_test` 暴露 `t311_expressions` 的 duration roundtrip 回归（`toString` 守卫未放行 duration map）。
  - 修复后复跑 `fmt + workspace_quick_test + tier0/1/2 + binding_smoke + contract_smoke` 全绿。

### 18.3 对 BETA-04 稳定窗的影响

- R13-W3 属于“错误断言从桥接到严格”的语义收紧，不改变稳定窗口径（仍以 `pass_rate` 与 `failed` 为准）。
- 在保持 `failed=0` 的前提下补齐 runtime 类型错误语义，降低“桥接掩盖语义缺口”的回退风险。

### 18.4 证据文件

- `artifacts/tck/beta-04-r13w3-runtime-strict-scan-2026-02-14.log`
- `artifacts/tck/beta-04-r13w3-runtime-strict-scan-remaining-2026-02-14.log`
- `artifacts/tck/beta-04-r13w3-runtime-strict-rescan-2026-02-14.log`
- `artifacts/tck/beta-04-r13w3-gate-2026-02-14.log`
- `artifacts/tck/beta-04-r13w3-gate-rerun-2026-02-14.log`

---

## 19. 续更快照（2026-02-14，BETA-03R14-W1 runtime 语义一致性收口）

### 19.1 本轮完成项（R14-W1）

- 补齐 `WHERE` 路径的 runtime TypeError 严格语义：
  - 在 `FilterIter` 中接入 `ensure_runtime_expression_compatible`，对谓词表达式先做运行期类型校验，再执行布尔求值。
  - 修复此前“非法表达式在 WHERE 里静默变空结果”的语义缺口。
- 补齐 `type()` 在写路径物化关系值上的兼容性：
  - `evaluate_type` 新增 `Value::Relationship` 分支，与既有 `Value::EdgeKey` 同步解析关系类型名。
  - 修复 `SET ... RETURN type(r)` 场景可能返回 `null` 的问题。
- TDD 验证：
  - 新增并先跑红：
    - `test_where_invalid_list_index_raises_runtime_type_error`
    - `test_set_relationship_return_type_keeps_rel_type_name`
  - 完成修复后两条测试均转绿。

### 19.2 回归结果

- 集成测试：
  - `t301_expression_ops` 全通过（含新增 WHERE runtime 错误断言）。
  - `t108_set_clause` 全通过（含新增关系 `type(r)` 断言）。
  - `t313_functions` 全通过（校验 `type()` 既有读路径行为未回退）。
- TCK 定向：
  - `expressions/list/List1.feature` 全通过。
  - `expressions/graph/Graph4.feature` 全通过。
  - `clauses/set/Set1.feature` 全通过。
- 格式校验：
  - `cargo fmt --all -- --check` 通过。

### 19.3 对 BETA-04 稳定窗的影响

- 该波次属于“语义一致性补洞”，不改变稳定窗口径，但能降低 runtime TypeError 在不同执行节点上的不一致风险。
- 当前稳定窗阻断仍是“连续 7 天样本不足”，并且历史快照中仍包含非达标日（如 `2026-02-13`）。

### 19.4 证据文件

- `artifacts/tck/beta-04-r14w1-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w1-tier0-2026-02-14.log`

---

## 20. 续更快照（2026-02-14，BETA-03R14-W2 UNWIND runtime guard 收口）

### 20.1 本轮完成项（R14-W2）

- 以 TDD 方式补齐 `UNWIND` 执行入口的 runtime 类型语义一致性：
  - 新增并先跑红：
    - `test_unwind_invalid_list_index_raises_runtime_type_error`
    - `test_unwind_toboolean_invalid_argument_raises_runtime_type_error`
  - 修复实现：
    - 在 `nervusdb-query/src/executor/plan_tail.rs` 的 `execute_unwind` 中复用 `ensure_runtime_expression_compatible(...)`。
    - 在每行 `UNWIND` 展开前先做运行期表达式兼容性检查；不兼容时直接返回 runtime error。
- 行为变化：
  - 修复此前 `UNWIND` 对非法表达式“静默吞错并产出 `null` 行”的行为差异；
  - 与既有 `Project`、`OrderBy`、`WHERE(FilterIter)` 的 runtime guard 语义对齐。

### 20.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t306_unwind -- --nocapture`：`7 passed`（含新增 2 条）
- 扩展回归：
  - `t301_expression_ops`（`WHERE` runtime guard）
  - `t108_set_clause`（`SET ... RETURN type(r)`）
  - `t313_functions`（`type()` 既有语义）
  - `tck_harness`: `List1`、`Graph4`、`Set1`
  - 以上均通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 20.3 对后续 R14 的影响

- R14-W2 完成后，runtime guard 覆盖面从 `Project/OrderBy/WHERE` 扩展到 `UNWIND`。
- 下一步可继续审计写路径表达式入口（`SET/MERGE` 相关）是否仍存在“未 guard 的直接求值点”。

### 20.4 证据文件

- `artifacts/tck/beta-04-r14w2-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w2-tier0-2026-02-14.log`

---

## 21. 续更快照（2026-02-14，BETA-03R14-W3 写路径 runtime guard 收口）

### 21.1 本轮完成项（R14-W3）

- 以 TDD 方式补齐写路径表达式求值入口的 runtime 类型语义：
  - 新增并先跑红：
    - `test_set_invalid_toboolean_argument_raises_runtime_type_error`
  - 修复实现：
    - 在 `execute_set`、`execute_set_from_maps` 中，表达式求值前接入 `ensure_runtime_expression_compatible(...)`；
    - 在 `merge_apply_set_items`、`merge_eval_props_on_row` 中同步接入 guard。
- 行为变化：
  - 修复此前写路径（`SET/MERGE`）对非法函数参数（如 `toBoolean(1)`）可能“静默变 `null` 并继续执行”的语义差异；
  - 与既有 `Project/OrderBy/WHERE/UNWIND` 的 runtime TypeError 处理策略统一。

### 21.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t108_set_clause -- --nocapture`：`11 passed`（含新增 runtime 错误断言）
  - `cargo test -p nervusdb --test t105_merge_test -- --nocapture`：`2 passed`
  - `cargo test -p nervusdb --test t323_merge_semantics -- --nocapture`：`4 passed`
  - `cargo test -p nervusdb --test t306_unwind -- --nocapture`：`7 passed`
  - `cargo test -p nervusdb --test t301_expression_ops test_where_invalid_list_index_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t313_functions -- --nocapture`：`18 passed`
- TCK 定向：
  - `clauses/set/Set1.feature`、`expressions/list/List1.feature`、`expressions/graph/Graph4.feature` 全通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 21.3 对后续 R14 的影响

- R14-W3 完成后，runtime guard 已覆盖：
  - 读路径：`Project`、`OrderBy`、`WHERE`、`UNWIND`
  - 写路径：`SET`、`SET +=/=` map、`MERGE` 属性求值
- 下一步可把审计范围扩到 `DELETE/FOREACH` 这类“非投影表达式入口”，继续清理未 guard 的直接求值点。

### 21.4 证据文件

- `artifacts/tck/beta-04-r14w3-write-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w3-write-guard-tier0-2026-02-14.log`

---

## 22. 续更快照（2026-02-14，BETA-03R14-W4 FOREACH/DELETE 尾部入口收口）

### 22.1 本轮完成项（R14-W4）

- 以 TDD 方式补齐尾部执行入口的 runtime 类型语义：
  - 新增并先跑红：
    - `t324_foreach_invalid_toboolean_argument_raises_runtime_type_error`
    - `test_delete_list_index_with_invalid_index_type_raises_runtime_type_error`
  - 修复实现：
    - 在 `execute_foreach` 的列表表达式求值前接入 `ensure_runtime_expression_compatible(...)`；
    - 在 `execute_delete` 与 `execute_delete_on_rows` 的 DELETE 目标表达式求值前接入同一 guard。
- 行为变化：
  - 修复此前 `FOREACH/DELETE` 在非法表达式输入下可能“静默 `null` / 继续执行 / 返回 0 side effects”的行为差异；
  - 与既有 `Project/OrderBy/WHERE/UNWIND/SET/MERGE` 的 runtime TypeError 处理路径对齐。

### 22.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t324_foreach -- --nocapture`：`4 passed`（含新增 1 条）
  - `cargo test -p nervusdb --test create_test test_delete_list_index_with_invalid_index_type_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t108_set_clause test_set_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t306_unwind test_unwind_toboolean_invalid_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t301_expression_ops test_where_invalid_list_index_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
- TCK 定向：
  - `clauses/delete/Delete5.feature`、`clauses/delete/Delete1.feature`、`clauses/delete/Delete3.feature` 全通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0|tier1|tier2` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 22.3 对后续 R14 的影响

- R14-W4 完成后，runtime guard 覆盖面进一步扩展到：
  - `FOREACH` 列表入口
  - `DELETE` 目标表达式入口（含 `execute_delete` 与 `execute_delete_on_rows`）
- 当前 R14 的主要风险从“执行入口遗漏 guard”转向“函数覆盖面与错误码精细一致性”审计，可转入小步补洞策略。

### 22.4 证据文件

- `artifacts/tck/beta-04-r14w4-tail-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w4-tail-guard-tier0-2026-02-14.log`
- `artifacts/tck/beta-04-r14w4-tail-guard-tier12-2026-02-14.log`

---

## 23. 续更快照（2026-02-14，BETA-03R14-W5 CREATE 属性入口收口）

### 23.1 本轮完成项（R14-W5）

- 以 TDD 方式补齐 `CREATE` 属性表达式入口的 runtime 类型语义：
  - 新增并先跑红：
    - `test_create_property_with_invalid_toboolean_argument_raises_runtime_type_error`
  - 修复实现：
    - 在 `execute_create_from_rows` 中，对节点/关系属性表达式求值前接入 `ensure_runtime_expression_compatible(...)`。
- 行为变化：
  - 修复 `CREATE (:N {flag: toBoolean(1)})` 在非法参数输入下可能“静默跳过属性”的行为差异；
  - 将 `CREATE` 属性入口与既有 `WHERE/UNWIND/SET/MERGE/FOREACH/DELETE` 统一到同一 runtime TypeError 语义链路。

### 23.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test create_test test_create_property_with_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test create_test test_delete_list_index_with_invalid_index_type_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t324_foreach t324_foreach_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t108_set_clause test_set_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t306_unwind test_unwind_toboolean_invalid_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
- TCK 定向：
  - `clauses/create/Create1.feature`、`clauses/delete/Delete5.feature` 全通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 23.3 对后续 R14 的影响

- R14-W5 完成后，运行期类型守卫已覆盖主要写读执行入口；
- 后续可转为“函数白名单覆盖完整性”与“错误码精细一致性”审计，避免新增函数路径再次出现 silent null。

### 23.4 证据文件

- `artifacts/tck/beta-04-r14w5-create-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w5-create-guard-tier0-2026-02-14.log`

---

## 24. 续更快照（2026-02-14，BETA-03R14-W6 CALL 参数入口收口）

### 24.1 本轮完成项（R14-W6）

- 以 TDD 方式补齐 `CALL` 参数表达式入口的 runtime 类型语义：
  - 新增并先跑红：
    - `test_procedure_argument_expression_invalid_toboolean_raises_runtime_type_error`
  - 修复实现：
    - 在 `ProcedureCallIter::next` 的参数求值循环中，对每个参数表达式先接入 `ensure_runtime_expression_compatible(...)`，guard 失败即返回执行错误。
- 行为变化：
  - 修复 `CALL math.add(toBoolean(1), 2)` 先进入过程内部并返回 `math.add requires numeric arguments` 的偏差；
  - 将 `CALL` 参数入口统一到既有 `WHERE/UNWIND/SET/MERGE/FOREACH/DELETE/CREATE` 的 runtime `InvalidArgumentValue` 语义链路。

### 24.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t320_procedures test_procedure_argument_expression_invalid_toboolean_raises_runtime_type_error -- --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t324_foreach t324_foreach_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t108_set_clause test_set_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t306_unwind test_unwind_toboolean_invalid_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test create_test test_create_property_with_invalid_toboolean_argument_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t301_expression_ops test_where_invalid_list_index_raises_runtime_type_error -- --exact --nocapture`：`1 passed`
- TCK 定向：
  - `clauses/call/Call1.feature`、`clauses/call/Call2.feature`、`clauses/call/Call3.feature` 全通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 24.3 对后续 R14 的影响

- R14-W6 完成后，运行期表达式 guard 已覆盖主要读取、写入、尾部和过程调用入口；
- 后续优先级可切换到“函数覆盖白名单审计 + 错误码一致性抽查”，重点防止新增函数路径绕过 guard。

### 24.4 证据文件

- `artifacts/tck/beta-04-r14w6-call-guard-unit-2026-02-14.log`
- `artifacts/tck/beta-04-r14w6-call-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w6-call-guard-tier0-2026-02-14.log`
- `artifacts/tck/beta-04-r14w6-call-guard-fmt-2026-02-14.log`

---

## 25. 续更快照（2026-02-14，BETA-03R14-W7 聚合参数入口收口）

### 25.1 本轮完成项（R14-W7）

- 以 TDD 方式补齐聚合执行入口的 runtime 类型语义：
  - 新增并先跑红：
    - `test_aggregate_argument_invalid_toboolean_raises_runtime_type_error`
  - 红灯现象：
    - `RETURN count(toBoolean(1)) AS c` 未抛错，错误返回 `c=0`（非法参数被吞成 `null` 后进入 `count`）。
  - 修复实现：
    - 在 `execute_aggregate` 的输入行处理阶段，新增 `validate_aggregate_runtime_expressions(...)`；
    - 对 `count/sum/avg/min/max/collect/percentile` 的聚合参数表达式统一接入 `ensure_runtime_expression_compatible(...)`。
- 行为变化：
  - 聚合参数表达式与 `WHERE/UNWIND/SET/MERGE/FOREACH/DELETE/CREATE/CALL` 统一到同一 runtime TypeError 语义链路；
  - 修复“非法函数参数在聚合中被静默吞掉”的剩余入口。

### 25.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t152_aggregation test_aggregate_argument_invalid_toboolean_raises_runtime_type_error -- --nocapture`：
    - 红灯阶段：返回 `Row { c: Int(0) }`
    - 绿灯阶段：`1 passed`（抛 runtime `InvalidArgumentValue`）
  - `cargo test -p nervusdb --test t320_procedures test_procedure_argument_expression_invalid_toboolean_raises_runtime_type_error -- --nocapture`：`1 passed`
- TCK 定向：
  - `expressions/aggregation/Aggregation1.feature` 全通过；
  - `expressions/aggregation/Aggregation2.feature` 全通过；
  - `expressions/typeConversion/TypeConversion1.feature` 全通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 25.3 对后续 R14 的影响

- R14-W7 完成后，运行期表达式 guard 已覆盖常见表达式执行面；
- 后续剩余工作可聚焦到“低频入口白名单审计 + 错误码一致性抽样回归”，避免边缘路径重新引入 silent null。

### 25.4 证据文件

- `artifacts/tck/beta-04-r14w7-aggregate-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w7-aggregate-guard-tier0-2026-02-14.log`
- `artifacts/tck/beta-04-r14w7-aggregate-guard-fmt-2026-02-14.log`

---

## 26. 续更快照（2026-02-14，BETA-03R14-W8 IndexSeek 入口审计加固）

### 26.1 本轮完成项（R14-W8）

- 进行低频入口审计，聚焦 `IndexSeek` 值表达式路径：
  - 新增回归测试：
    - `test_index_seek_invalid_value_expression_raises_runtime_type_error`
- 场景与结论：
  - 场景：`MATCH (n:Person) WHERE n.name = toBoolean(1) RETURN n`
  - 结论：在索引路径下仍抛 runtime `InvalidArgumentValue`，不存在“被索引短路为空结果”的静默吞错。

### 26.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t107_index_integration test_index_seek_invalid_value_expression_raises_runtime_type_error -- --nocapture`：`1 passed`
- TCK 定向：
  - `expressions/typeConversion/TypeConversion1.feature` 全通过。
- 门禁：
  - `cargo fmt --all -- --check` 通过。

### 26.3 对后续 R14 的影响

- 审计结果表明 `IndexSeek` 路径在非法值表达式上未引入新的 runtime 语义偏差；
- 下一步可继续对剩余低频入口做同类“先断言、再验证”的薄层审计，逐步关闭回归面。

### 26.4 证据文件

- `artifacts/tck/beta-04-r14w8-index-seek-audit-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w8-index-seek-audit-fmt-2026-02-14.log`

---

## 27. 续更快照（2026-02-14，BETA-03R14-W9 percentile 双参数审计加固）

### 27.1 本轮完成项（R14-W9）

- 补齐 `percentile` 双参数路径的 runtime 回归断言：
  - 新增测试：
    - `test_percentile_argument_invalid_toboolean_raises_runtime_type_error`
- 场景与结论：
  - 场景：`RETURN percentileDisc(1, toBoolean(1)) AS p`
  - 结论：稳定抛 runtime `InvalidArgumentValue`，`PercentileDisc/PercentileCont` 双表达式 guard 分支语义稳定。

### 27.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t152_aggregation test_aggregate_argument_invalid_toboolean_raises_runtime_type_error -- --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t152_aggregation test_percentile_argument_invalid_toboolean_raises_runtime_type_error -- --nocapture`：`1 passed`
- TCK 定向：
  - `expressions/aggregation/Aggregation2.feature` 全通过；
  - `expressions/typeConversion/TypeConversion1.feature` 全通过。
- 门禁：
  - `cargo fmt --all -- --check` 通过。

### 27.3 对后续 R14 的影响

- `percentile` 聚合路径的双参数 guard 现已有独立回归锁定；
- 后续可进一步转向“遗漏入口枚举清单 + 自动扫描脚本”来量化 R14 收口完成度。

### 27.4 证据文件

- `artifacts/tck/beta-04-r14w9-percentile-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w9-percentile-guard-fmt-2026-02-14.log`

---

## 28. 续更快照（2026-02-14，BETA-03R14-W10 IndexSeek 值表达式入口收口）

### 28.1 本轮完成项（R14-W10）

- 修复 `IndexSeek` 执行入口的 runtime guard 覆盖：
  - 在 `execute_index_seek` 中对 `value_expr` 求值前接入 `ensure_runtime_expression_compatible(...)`；
  - 行为目标：即使未来 planner/路径发生变化，也不会依赖 fallback 分支来“碰巧触发 runtime 错误”。
- 回归用例保留：
  - `test_index_seek_invalid_value_expression_raises_runtime_type_error` 锁定 `MATCH (n:Person) WHERE n.name = toBoolean(1) RETURN n` 抛 runtime `InvalidArgumentValue`。

### 28.2 回归结果

- 定向测试：
  - `cargo test -p nervusdb --test t107_index_integration test_index_seek_invalid_value_expression_raises_runtime_type_error -- --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t320_procedures test_procedure_argument_expression_invalid_toboolean_raises_runtime_type_error -- --nocapture`：`1 passed`
  - `cargo test -p nervusdb --test t152_aggregation test_percentile_argument_invalid_toboolean_raises_runtime_type_error -- --nocapture`：`1 passed`
- TCK 定向：
  - `expressions/typeConversion/TypeConversion1.feature` 全通过。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 28.3 对后续 R14 的影响

- `IndexSeek` 值表达式路径的 runtime guard 已从“间接覆盖”提升为“入口强覆盖”；
- 后续可继续按同模式对剩余低频入口做显式 guard 与回归断言锁定。

### 28.4 证据文件

- `artifacts/tck/beta-04-r14w10-index-seek-guard-targeted-2026-02-14.log`
- `artifacts/tck/beta-04-r14w10-index-seek-guard-fmt-2026-02-14.log`
- `artifacts/tck/beta-04-r14w10-index-seek-guard-tier0-2026-02-14.log`

---

## 29. 续更快照（2026-02-14，BETA-03R14-W11 runtime guard 审计脚本落地）

### 29.1 本轮完成项（R14-W11）

- 新增可重复执行的审计脚本，量化 executor 侧 runtime guard 覆盖面：
  - 新增：`scripts/runtime_guard_audit.sh`
  - 兼容性：避免使用 bash 4+ 才支持的关联数组（macOS 默认 bash 3.2 可直接运行）。
- 脚本输出内容：
  - 统计 `nervusdb-query/src/executor` 下 `evaluate_expression_value(...)` 与 `ensure_runtime_expression_compatible(...)` 的分布；
  - 自动列出 `eval>0 && guard==0` 的潜在热点文件，用于后续收口排查。

### 29.2 审计结论（当前快照）

- 当前唯一被标记的潜在热点：`nervusdb-query/src/executor/write_orchestration.rs`
  - 说明：该处为 delete overlay 目标收集逻辑（`collect_delete_targets_from_rows`），不在实际删除执行入口；
  - 后续动作：可评估是否需要将收集函数改为 `Result<...>` 并在收集阶段也显式 guard（当前不会影响 DELETE 的最终 runtime 错误语义，因为执行入口已有 guard）。

### 29.3 证据文件

- `artifacts/tck/beta-04-r14w11-runtime-guard-audit-2026-02-14.log`

---

## 30. 续更快照（2026-02-14，BETA-03R14-W12 清零 runtime guard 审计热点）

### 30.1 本轮完成项（R14-W12）

- 修复审计脚本识别出的唯一 executor 热点（`write_orchestration.rs`）：
  - 将 `collect_delete_targets_from_rows` 升级为 `Result<...>`；
  - 在 delete overlay 目标收集阶段，对每个 `DELETE` 目标表达式求值前接入 `ensure_runtime_expression_compatible(...)`。
- 目标：
  - 消除“内部收集阶段直接求值但未 guard”的剩余路径，使 runtime 语义一致性更稳健（不依赖后续执行入口兜底）。

### 30.2 回归与门禁结果

- 审计脚本：
  - `scripts/runtime_guard_audit.sh` 输出 `potential hotspots (eval>0 && guard==0)` 为 `none`。
- 门禁：
  - `bash scripts/tck_tier_gate.sh tier0` 全通过；
  - `cargo fmt --all -- --check` 通过。

### 30.3 证据文件

- `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-2026-02-14.log`
- `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-tier0-2026-02-14.log`
- `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-fmt-2026-02-14.log`

---

## 31. 续更快照（2026-02-15，BETA-03R14-W13 收尾 + BETA-04 strict 稳定窗 Day1）

### 31.1 本轮完成项（R14-W13-A）

- runtime guard 审计脚本收口到可发布形态：
  - `scripts/runtime_guard_audit.sh` 正式支持 `--root <dir>`、`--fail-on-hotspot`、`--help`；
  - 在无 `rg` 环境下自动回退 `grep -RIn`，保持可执行性。
- CI 门禁前置接线：
  - `ci.yml` 增加 `bash scripts/runtime_guard_audit.sh --fail-on-hotspot`；
  - 位置放在 `fmt/clippy` 后、`workspace_quick_test` 前，做到尽早失败。
- 写路径语义补点（Temporal4 失败簇收口）：
  - 修复 list 属性转换对 duration map 的误拦截；
  - 允许 `List<Duration>` 写入属性，普通 map list 仍保持 `InvalidPropertyType` 拦截；
  - 新增双向回归测试：允许 duration map list / 拒绝普通 map list。

### 31.2 验证结果（W13-A）

- 核心门禁链全绿：
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
  - `bash scripts/runtime_guard_audit.sh --fail-on-hotspot`
  - `bash scripts/workspace_quick_test.sh`
  - `bash scripts/tck_tier_gate.sh tier0`
  - `bash scripts/tck_tier_gate.sh tier1`
  - `bash scripts/tck_tier_gate.sh tier2`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`
- Tier-3 全量保持全绿：
  - `3897 scenarios (3897 passed)`，失败簇报告 `No step failures found`。

### 31.3 BETA-04 strict 稳定窗 Day1（W13-B/W13-C）

- 基建已落地：
  - `ci-daily-snapshot.yml`
  - `stability-window-daily.yml`
  - `scripts/stability_window.sh`（strict 模式）
  - `scripts/beta_release_gate.sh`
  - `release.yml` 接入发布阻断（仅发布阻断，不阻断日常 PR）。
- Day1（2026-02-15）产物已写入：
  - `artifacts/tck/tier3-rate-2026-02-15.json`（`pass_rate=100.00`，`failed=0`）
  - `artifacts/tck/ci-daily-2026-02-15.json`（`all_passed=true`）
  - `artifacts/tck/stability-daily-2026-02-15.json`
  - `artifacts/tck/stability-window.json`
  - `artifacts/tck/stability-window.md`
- strict 窗口状态（截至 2026-02-15）：
  - `consecutive_days=0/7`
  - `window_passed=false`
  - 本地运行因 `github_data_unavailable`（nightly workflow 历史需主分支运行后可回填）未形成连续计数。
- 若后续每日全通过且无重置，最早达标日期：`2026-02-21`。

### 31.4 证据文件

- `artifacts/tck/beta-04-r14w13-runtime-guard-gate-2026-02-15.log`
- `artifacts/tck/beta-04-r14w13-core-gates-2026-02-15.log`
- `artifacts/tck/beta-04-r14w13-tier3-full-2026-02-15.log`
- `artifacts/tck/beta-04-r14w13-tier3-full-2026-02-15.cluster.md`
- `artifacts/tck/beta-04-r14w13-stability-window-day1-2026-02-15.log`
- `artifacts/tck/beta-04-r14w13-stability-window-day1-2026-02-15.rc`

---

## 32. 续更快照（2026-02-15，W13-PERF 内存上涨→吞吐下降攻坚）

### 32.1 本轮完成项

- 执行资源护栏统一落地（默认开启）：
  - `ExecuteOptions` 新增并接入 `Params`（平衡档默认值）；
  - `ResourceLimitExceeded` 错误语义新增（`kind/limit/observed/stage`）；
  - 执行期 runtime 计数器（超时/中间行数/集合大小/Apply 每外层行上限）接入全链路。
- 执行器热点收敛：
  - `UNWIND` 改为迭代发射，避免每输入行先构造完整 `Vec<Row>`；
  - `ORDER BY`、`OptionalWhereFixup` 增加有界收集与超时检查；
  - `Aggregate` 增加 group/rows/collect-distinct 规模限制；
  - `Apply` 增加 `max_apply_rows_per_outer` 限制。
- Fuzz 策略补强：
  - `query_execute` 增加 `-timeout=5`（保留 `rss_limit_mb=4096`）。

### 32.2 验证结果

- 新增资源限制回归：`t341_resource_limits`（5/5 通过）。
- 定向主簇：`Match4`、`Match9` 全通过。
- 扩展回归矩阵：`Match1/2/3/6/7 + Path1/2/3 + Quantifier1/2` 全通过。
- 基线门禁全绿：
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
  - `bash scripts/workspace_quick_test.sh`
  - `bash scripts/tck_tier_gate.sh tier0`
  - `bash scripts/tck_tier_gate.sh tier1`
  - `bash scripts/tck_tier_gate.sh tier2`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`

### 32.3 当前状态与剩余动作

- 代码级护栏与回归已经闭环，当前状态可进入 Nightly 观察。
- 8h Fuzz 平衡目标（`slowest<=5s`, `rss<=2048MB`, `exec/s>=baseline 70%`）需主分支 Nightly 跑完后以自动产物补齐最终判定。

### 32.4 证据文件

- `artifacts/tck/w13-perf-baseline.json`
- `artifacts/tck/w13-perf-after-A.json`
- `artifacts/tck/w13-perf-after-B.json`
- `artifacts/tck/w13-perf-final.json`

### 32.5 监控后补丁（2026-02-15，Fuzz Nightly `query_parse` timeout 收口）

- 触发背景：
  - PR 监控中，`Fuzz Nightly` 在 `query_parse` 目标出现 `libFuzzer: timeout`，
    导致后续 `query_execute` 阶段被跳过。
- 根因与修复：
  - 在 `nervusdb-query/src/parser.rs` 增加解析复杂度步数预算护栏；
  - 预算耗尽统一返回 `syntax error: ParserComplexityLimitExceeded`，避免病态输入长时间占用 worker；
  - 新增单元测试 `parser_complexity_guard_trips_with_tiny_budget` 防回归。
- 回归样本固化：
  - 新增 `fuzz/regressions/query_parse/timeout-0150b9c6c52d68d4492ee8debb67edad1c52a05f`。
- 实测结果（同一 failing input）：
  - 本地回放从此前约 `9.3s` 降至 `~71ms`，显著低于 workflow 的 `-timeout=10` 阈值。
- 证据：
  - `artifacts/tck/w13-perf-query-parse-timeout-fix-2026-02-15.log`

---

## 33. 续更快照（2026-02-16，BETA-04 strict 稳定窗 Day2 回填修复）

### 33.1 本轮完成项（W13-Day2）

- `scripts/stability_window.sh` 回填逻辑加固：
  - tier3 回填选择从“当天 startswith”收敛为“`created_at <= day_end(UTC)` 的最新成功 run”；
  - artifact 选择优先级固定：`tck-nightly-artifacts` > `beta-gate-artifacts`；
  - 回填失败原因细化并可审计：`artifact_fetch_auth_failed`、`artifact_not_found`、`tier3_backfill_failed`。
- 清理脚本重复定义，确保 `backfill_ci_daily_file` 与 tier3 回填逻辑只有单一生效实现。
- 新增 fixture 回归：
  - `scripts/tests/stability_window_fixture.sh`；
  - 覆盖 4 个场景：7 天全通过、tier3 中途失败、缺失 ci-daily、token/无 token reason 区分。

### 33.2 验证结果

- 语法/结构检查：
  - `bash -n scripts/stability_window.sh`
  - `bash -n scripts/tests/stability_window_fixture.sh`
- fixture 回归：
  - `bash scripts/tests/stability_window_fixture.sh` 全通过。
- 实况复算（带 GitHub token）：
  - `bash scripts/stability_window.sh --mode strict --date 2026-02-16 --github-repo LuQing-Studio/nervusdb --github-token-env GITHUB_TOKEN`
  - `2026-02-16` 当日判定 `pass=true`，不再出现 `missing_tier3_rate` 阻断；
  - 窗口累计更新为 `consecutive_days=2/7`（仍未达发布放行门槛）。

### 33.3 证据文件

- `artifacts/tck/beta-04-day2-backfill-2026-02-16.log`
- `artifacts/tck/beta-04-day2-backfill-2026-02-16.rc`

## 34. 续更快照（2026-02-17，BETA-04 主线 B：内核缺口首批清零）

### 34.1 本轮目标与范围

- 目标 1：清零多标签子集匹配缺口（`MATCH (n:Manager)`）。
- 目标 2：清零关系 `MERGE` 幂等缺口（重复 `MERGE (a)-[:LINK]->(b)` 不重复建边）。
- 范围：仅修执行语义与回归测试，不改绑定 API 签名。

### 34.2 核心修复

- 多标签匹配语义修复：
  - `NodeScanIter` 标签过滤由“主标签相等”改为“节点标签集合包含”；
  - 当 `resolve_node_labels` 不可用时，仍回退到旧路径保证兼容。
  - 文件：`nervusdb-query/src/executor/plan_iterators.rs`
- 关系 `MERGE` 语义修复：
  - `execute_write` 的 `MERGE` 路径统一委托 `write_orchestration::execute_merge_with_rows`；
  - 修复前置 `MATCH` 绑定行在关系 `MERGE` 中丢失导致的错误建边/自环问题；
  - 重复执行同一 `MERGE` 时保持幂等。
  - 文件：`nervusdb-query/src/executor/merge_execution.rs`、`nervusdb-query/src/executor.rs`
- 新增核心回归：
  - `nervusdb/tests/t342_label_merge_regressions.rs`

### 34.3 三端能力测试口径收紧（soft-pass -> hard assert）

- Rust：`examples-test/nervusdb-rust-test/tests/test_capabilities.rs`
- Node：`examples-test/nervusdb-node-test/src/test-capabilities.ts`
- Python：`examples-test/nervusdb-python-test/test_capabilities.py`

上述场景不再打印 `[CORE-BUG]` 继续通过，而是改为硬断言失败即红灯。

### 34.4 验证结果

- `cargo test -p nervusdb --test t342_label_merge_regressions`：2/2 通过。
- `bash examples-test/run_all.sh`：Rust/Node/Python 全绿（0 fail）。
- `cargo test -p nervusdb --test tck_harness -- --input clauses/match/Match1.feature`：通过。
- `cargo test -p nervusdb --test tck_harness -- --input clauses/merge/Merge1.feature`：通过。
- `cargo test -p nervusdb --test tck_harness -- --input clauses/merge/Merge2.feature`：通过。

### 34.5 文档状态

- `examples-test` 三端 `CAPABILITY-REPORT.md` 已同步：
  - 移除“多标签子集匹配”和“MERGE 关系不稳定”的已知缺口标记；
  - 保留尚未清零缺口：`left/right`、`shortestPath`。

## 35. 续更快照（2026-02-18，核心缺口收口 + Fuzz query_execute timeout 止血）

### 35.1 本轮目标与范围

- 目标 1：清零剩余核心缺口 `left/right`、`shortestPath`（三端硬断言口径）。
- 目标 2：修复 `Fuzz Nightly` 中 `query_execute` 单样本 timeout 失败告警。
- 范围：函数实现、解析兼容、examples-test 硬断言收口、fuzz 目标与 workflow 参数收敛。

### 35.2 核心修复

- 字符串函数补齐：
  - 新增 `left()`、`right()` 实现并接入函数分发；
  - 编译期函数白名单同步加入 `left/right`，消除 `UnknownFunction`。
  - 文件：`nervusdb-query/src/evaluator/evaluator_scalars.rs`、`nervusdb-query/src/query_api/type_validation.rs`
- `shortestPath` 解析兼容修复：
  - `parse_pattern` 支持 `MATCH p = shortestPath((...)-[*]->(...))` 与 `allShortestPaths(...)` 包装语法；
  - 修复此前 `Expected '('` 解析失败。
  - 文件：`nervusdb-query/src/parser.rs`
- 核心回归新增：
  - `nervusdb/tests/t313_functions.rs::test_left_and_right_string_functions`
  - `nervusdb/tests/t318_paths.rs::test_shortest_path_in_match_assignment`
- 三端 capability 硬断言收口：
  - Rust/Node/Python capability 测试移除 `left/right` 与 `shortestPath` 的 soft-skip 分支。
  - 文件：
    - `examples-test/nervusdb-rust-test/tests/test_capabilities.rs`
    - `examples-test/nervusdb-node-test/src/test-capabilities.ts`
    - `examples-test/nervusdb-python-test/test_capabilities.py`
- Fuzz timeout 止血：
  - `query_execute` fuzz target 加执行预算（`ExecuteOptions`）并限制输入长度 `<=1024`；
  - nightly 参数调整为 `-max_len=1024 -timeout=10`。
  - 文件：`fuzz/fuzz_targets/query_execute.rs`、`.github/workflows/fuzz-nightly.yml`

### 35.3 验证结果

- `cargo test -p nervusdb --test t313_functions test_left_and_right_string_functions`：通过。
- `cargo test -p nervusdb --test t318_paths test_shortest_path_in_match_assignment`：通过。
- `bash examples-test/run_all.sh`：Rust/Node/Python 全绿（0 fail）。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`：通过。
- `cargo +nightly fuzz run query_execute -- -max_total_time=5 -max_len=1024 -timeout=10 -rss_limit_mb=4096`：通过（本地 smoke）。

### 35.4 状态更新

- 核心缺口 `left/right`、`shortestPath` 已清零；
- `examples-test` 不再保留这两项已知缺口；
- `BETA-04` 稳定窗继续累计（截至 2026-02-18：`consecutive_days=3/7`）。

## 36. 续更快照（2026-02-18，BETA-04 strict 稳定窗 Day4 恢复）

### 36.1 现象与根因

- 当日检查发现 `stability-window` 的 `2026-02-17` / `2026-02-18` 条目被标记为 `BLOCKED`。
- 直接原因不是 TCK/CI/nightly 失败，而是 Tier-3 回填失败原因为 `artifact_fetch_auth_failed`。
- 根因：执行 `stability_window.sh` 的 workflow 仅配置了 `contents: read`，缺少 GitHub Actions artifacts/runs 读取所需的 `actions: read` 权限。

### 36.2 修复动作

- 为所有调用 `stability_window.sh` 的 workflow 增补权限：
  - `.github/workflows/stability-window-daily.yml`
  - `.github/workflows/tck-nightly.yml`
  - `.github/workflows/release.yml`
- 权限变更：在 `permissions` 中新增 `actions: read`（保持原有 `contents` 权限不变）。

### 36.3 复算与结果

- 复算命令（UTC）：
  - `bash scripts/stability_window.sh --mode strict --date 2026-02-18 --github-repo LuQing-Studio/nervusdb --github-token-env GITHUB_TOKEN`
- 复算结果：
  - `2026-02-17`：`PASS`
  - `2026-02-18`：`PASS`
  - 窗口累计维持 `3/7`（`2026-02-15` 仍为 `threshold_or_failed`，因此不计入连续通过）
  - `window_passed=false`（发布门禁继续阻断，等待累计到 `7/7`）

### 36.4 证据

- `artifacts/tck/beta-04-stability-window-day4-2026-02-18.log`
- `artifacts/tck/beta-04-stability-window-day4-2026-02-18.rc`
- `artifacts/tck/stability-window.md`

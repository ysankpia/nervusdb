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
| TCK 文件名清理（tXXX_ 前缀） | 未执行 | 依赖 TCK 100% 通过后执行（当前 84.83%） |

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
| TCK Tier-3 全量通过率 | ≥95% | 84.83%（3306/3897） | 差距 ~10pp |
| 连续 7 天稳定窗 | 7 天全绿 | 未启动（BETA-04 Plan） | 阻塞于 TCK |
| 性能 SLO 封板 | P99 读≤120ms/写≤180ms/向量≤220ms | 未启动（BETA-05 Plan） | 阻塞于稳定窗 |

### 3.2 TCK 收敛进展

| 日期 | 通过 | 总数 | 通过率 | 失败 | 变化 |
|------|------|------|--------|------|------|
| 2026-02-10 | 2989 | 3897 | 76.70% | — | 基线 |
| 2026-02-11 | 3193 | 3897 | 81.93% | 178 | +204 场 |
| 2026-02-13 | 3306 | 3897 | 84.83% | 56 | +113 场（较 2026-02-11） |

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
| TCK Tier-3 通过率 | 84.83%（3306/3897） | `artifacts/tck/tier3-rate-2026-02-13.md` |
| TCK 失败场景数 | 56 | `artifacts/tck/tier3-rate-2026-02-13.md` |
| NotImplemented 残留 | 8 处 | grep 验证 |
| executor/ 文件数 | 34 | `nervusdb-query/src/executor/` |
| evaluator/ 文件数 | 25 | `nervusdb-query/src/evaluator/` |
| 包名 -v2 残留 | 0 | 所有 Cargo.toml 已清理 |
| CLI 对 storage 直接依赖 | 0 | grep 验证 |
| Phase 2-4 文件存在性 | 0（buffer_pool/vfs/label_index/compaction 均不存在） | 文件系统检查 |

---

## 5. 下一步建议

### 短期（当前冲刺）
- 继续 BETA-03（Tier-3 全量）失败簇修复，目标从 84.83% → ≥95%
- 消除剩余 8 个 NotImplemented（优先处理影响 TCK 通过率的项）

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

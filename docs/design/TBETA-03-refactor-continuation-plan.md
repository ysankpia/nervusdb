# TBETA-03 实施计划：重构后续推进（Refactor-First）

## 1. 背景与目标

当前 `nervusdb-query` 核心文件体量已进入高风险区：
- `nervusdb-query/src/executor.rs`: 6524 行
- `nervusdb-query/src/evaluator.rs`: 4832 行
- `nervusdb-query/src/query_api.rs`: 4187 行

目标：先做受控重构，再继续 TCK 清簇，避免在超大单文件持续叠加导致回归半径扩大。

## 2. 范围约束（硬约束）

- 不修改公共 API（Rust/CLI/Python/Node）签名。
- 不改 `storage_format_epoch` 与兼容语义。
- 重构 PR 只做结构拆分，语义必须等价。
- 每个 PR 严格执行短门禁与目标 feature 回归。

## 3. 分阶段执行（短 PR）

### R1：拆分 `query_api.rs`
- 分支：`codex/feat/TBETA-03-refactor-query-api`
- 目标：
  - 拆成解析、语义校验、Plan 组装子模块。
  - 把 `RETURN/ORDER BY` 聚合与作用域校验逻辑单独归档，减少跨功能耦合。
- 验收：
  - `ReturnOrderBy2.feature` 至少保持现状不新增失败。
  - tier0/tier1/tier2 + quick + bindings + contract 全绿。

### R2：拆分 `executor.rs`
- 分支：`codex/feat/TBETA-03-refactor-executor`
- 目标：
  - 拆出 `write` 子模块（SET/DELETE/MERGE）。
  - 拆出 `read/projection/sort` 子模块，降低排序与聚合改动冲击范围。
- 验收：
  - Delete/Set/Merge 既有定向回归保持通过。
  - Tier 门禁不回退。

### R3：拆分 `evaluator.rs`
- 分支：`codex/feat/TBETA-03-refactor-evaluator`
- 目标：
  - 提取 `temporal` 与 `duration` 子模块。
  - 统一 temporal helper 入口，减少 `date/time/datetime/duration` 分叉漂移。
- 验收：
  - Temporal1/3/8/10/6 定向回归不回退。
  - `t311_expressions` 全绿。

### R4：恢复清簇推进
- 分支：`codex/feat/TBETA-03-returnorderby2-fixes`（当前进行中）
- 目标：
  - 清零 `ReturnOrderBy2` 的 2 个失败：
    1. `Count star should count everything in scope`（误报 `InvalidAggregation`）
    2. `DISTINCT` 后 `ORDER BY` 变量作用域应报 `UndefinedVariable`
  - 然后继续 Wave2 余簇。

## 4. 测试与门禁（每个短 PR）

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
3. `bash scripts/workspace_quick_test.sh`
4. `bash scripts/tck_tier_gate.sh tier0`
5. `bash scripts/tck_tier_gate.sh tier1`
6. `bash scripts/tck_tier_gate.sh tier2`
7. `bash scripts/binding_smoke.sh`
8. `bash scripts/contract_smoke.sh`
9. 对应 feature 定向回归（必跑）

## 5. 当前进度（2026-02-11）

- 已完成并合并：PR #126、#127、#128。
- Tier-3 最新：`3193/3897=81.93%`（`failed=178`）。
- 进行中：`R4`（`ReturnOrderBy2` 尚余 2 失败）。
- 已记录并同步：`docs/tasks.md` 的 BETA-03 与 BETA-03R1~R4。

## 6. 风险与控制

- 风险：重构引入隐式语义漂移，导致 TCK 局部回退。
- 控制：
  - 严格“结构拆分优先，行为不变”。
  - 每个 PR 用定向 feature + tier0/1/2 兜底。
  - 出现回退只回滚当个短分支，禁止跨 PR 混改。

# NervusDB 项目实现报告（阶段快照）

- 生成时间：2026-02-11 18:34:42 CST
- 仓库路径：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb`
- 当前分支：`codex/feat/TBETA-03-returnorderby2-fixes`
- 用途：给重启后的续做提供单文件恢复入口

## 1. 今日已落地并合并到 `main` 的实现

### PR #126
- 标题：`feat(TBETA-03): fix comparison chain semantics`
- 结果：`Comparison3/4` 清零。

### PR #127
- 标题：`feat(TBETA-03): implement SET map semantics for Set4/Set5`
- 结果：`SET n = {...}` 与 `SET n += {...}` 语义全链路支持，`Set4/Set5` 结果断言通过（side-effects 相关步骤仍为 harness skip）。

### PR #128
- 标题：`feat(TBETA-03): fix DELETE compile validation and null semantics`
- 关键改动：
  - `nervusdb-query/src/query_api.rs`
  - `nervusdb-query/src/executor.rs`
  - `nervusdb/tests/create_test.rs`
- 修复点：
  - `DELETE` 编译期校验补全：`UndefinedVariable`、`InvalidDelete`、`InvalidArgumentType`。
  - `DELETE` 支持表达式求值后的实体删除（node/relationship/path/list/map）。
  - `OPTIONAL MATCH ... DELETE ... RETURN` 的 `null` 行保留，不再报 `Variable ... not found in row`。
- 本地门禁（本轮）：
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
  - `bash scripts/workspace_quick_test.sh`
  - `bash scripts/tck_tier_gate.sh tier0`
  - `bash scripts/tck_tier_gate.sh tier1`
  - `bash scripts/tck_tier_gate.sh tier2`
  - `bash scripts/binding_smoke.sh`
  - `bash scripts/contract_smoke.sh`
  - 定向回归：`Delete1.feature`、`Delete2.feature`、`Delete5.feature`

## 2. 最新全量快照与看板现状

- `artifacts/tck/tier3-rate.md` 当前为：
  - `3193/3897 = 81.93%`
  - `failed = 178`
  - `skipped = 526`
- `artifacts/tck/tier3-cluster.md` 当前 Top 失败簇含：
  - `Comparison3`（8）
  - `Temporal5`（7）
  - `Temporal2`（7）
  - `Match9`（7）
  - `Match4`（7）
  - `ReturnOrderBy2`（2）
- `docs/tasks.md` 中 `BETA-03` 记录仍是 `3105/3897=79.68%`，已落后于最新快照，后续需同步。

## 3. 当前分支与工作区状态（重启前）

- 分支：`codex/feat/TBETA-03-returnorderby2-fixes`
- 已跟踪文件：干净（无已修改 tracked 文件）
- 未跟踪但需要保留（用户指定）：
  - `docs/hypothetical-architecture/`
  - `nervusdb-Architecture.md`

## 4. 正在进行中的目标（下一步直接续做）

目标 feature：`clauses/return-orderby/ReturnOrderBy2.feature`

当前剩余失败 2 个 scenario：
1. Scenario `[6]` `Count star should count everything in scope`
   - 现象：`Query failed: Some("syntax error: InvalidAggregation")`
2. Scenario `[13]` `Fail when sorting on variable removed by DISTINCT`
   - 现象：期望 compile-time `UndefinedVariable`，当前是 success

结论：下一步集中修 `RETURN/ORDER BY` 的聚合校验与 `DISTINCT` 作用域变量可见性校验。

## 5. 重启后恢复执行清单（可直接复制）

1. `cd /Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb`
2. `git status --short --branch`
3. `git checkout codex/feat/TBETA-03-returnorderby2-fixes`
4. `cargo test -p nervusdb --test tck_harness -- --input clauses/return-orderby/ReturnOrderBy2.feature`
5. 修复后按短 PR 门禁跑：
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
   - `bash scripts/workspace_quick_test.sh`
   - `bash scripts/tck_tier_gate.sh tier0`
   - `bash scripts/tck_tier_gate.sh tier1`
   - `bash scripts/tck_tier_gate.sh tier2`
   - `bash scripts/binding_smoke.sh`
   - `bash scripts/contract_smoke.sh`
   - `cargo test -p nervusdb --test tck_harness -- --input clauses/return-orderby/ReturnOrderBy2.feature`
6. 提交/PR：
   - `git push -u origin codex/feat/TBETA-03-returnorderby2-fixes`
   - `gh pr create --base main --head codex/feat/TBETA-03-returnorderby2-fixes`
   - `gh pr checks <PR号> --watch`
   - `gh pr merge <PR号> --squash --delete-branch`

## 6. 备注

- 当前仓库没有触发任何需要“核按钮确认”的操作。
- 本报告不改动 `spec.md`，仅用于阶段交接与恢复执行。
- 重构后续执行计划已落盘：`docs/design/TBETA-03-refactor-continuation-plan.md`。
- 任务板进度已同步：`docs/tasks.md` 中 `BETA-03` 已更新到 `3193/3897=81.93%`，并新增 `BETA-03R1~R4` 重构/恢复任务。

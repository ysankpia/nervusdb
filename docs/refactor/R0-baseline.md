# R0：审计基线与行为等价护栏

更新时间：2026-02-11  
任务类型：Phase 0（前置）  
任务状态：Done

## 1. 目标

- 冻结当前可执行事实基线，作为后续所有重构任务的对照标准。
- 固化“行为等价”判定口径，避免结构拆分混入语义变更。
- 为 R1/R2/R3/S1/S2/S3 提供统一门禁入口与回滚准则。

## 2. 边界

- 仅新增 `docs/refactor` 文档，不改生产代码。
- 不调整 `docs/spec.md` 与 `docs/tasks.md` 现有定义。
- 不修改 `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-Architecture.md`。

## 3. 基线指标（2026-02-11）

| 指标 | 当前值 | 证据 |
|---|---|---|
| Tier-3 通过率 | 81.93%（3193/3897） | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:11` |
| Tier-3 失败数 | 178 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:9` |
| Query API 文件规模 | 4187 行 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:4187` |
| Executor 文件规模 | 6524 行 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:6524` |
| Evaluator 文件规模 | 4832 行 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/evaluator.rs:4832` |
| CLI 直连 Storage 依赖 | 存在 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:6`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:5` |
| 必跑门禁集合 | 5 项已定义 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:38`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:42` |

## 4. 行为等价护栏清单

以下清单是每个重构 PR 的“最小回归面”：

1. Query 解析/校验/执行主路径：
   - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t52_query_api.rs`
   - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t311_expressions.rs`
   - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t333_varlen_direction.rs`
2. 排序与分页语义：
   - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t62_order_by_skip_test.rs`
3. 跨语言契约：
   - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t332_binding_validation.rs`
   - `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/tests/test_basic.py`
4. 当前已知失败簇（后置处理）：
   - ReturnOrderBy2 仍有 2 个失败（`InvalidAggregation` / `UndefinedVariable`）
   - 证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:103`

## 5. 每 PR 执行命令（硬阻断）

```bash
cargo fmt --all -- --check
cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
bash scripts/tck_tier_gate.sh tier0
bash scripts/tck_tier_gate.sh tier1
bash scripts/tck_tier_gate.sh tier2
bash scripts/binding_smoke.sh
bash scripts/contract_smoke.sh
```

证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:38` 到 `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:42`

## 6. 回滚步骤

1. 任一 PR 触发 P0 行为变化，立即 `git revert <merge_commit>`。
2. 任一 PR 门禁失败且无法在同 PR 修复，直接回滚，不允许跨任务补丁掩盖。
3. 回滚后重新执行 Phase 0 命令集，恢复到最近一次全绿快照。

## 7. 完成定义（DoD）

- 所有 P0/P1 断言均有 `文件:行号` 证据。
- 后续任务（R1-R3/S1-S3）均具备独立任务文档和门禁列表。
- 重构过程中的每个 PR 都可与本基线进行差分。

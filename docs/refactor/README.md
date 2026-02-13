# NervusDB 全面重构审计总览（Phase 0 基线）

更新时间：2026-02-13  
执行分支：`codex/feat/R0-refactor-baseline`

## 1. 适用范围

- 本文是重构审计映射总表，服务于 4-6 周串行重构执行。
- 事实裁决源固定为 `代码 + tests + artifacts`，高于叙事文档。
- 本文不修改 `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-Architecture.md`，仅做证据化映射。

## 2. 不变约束

- 对外 API 不变（Rust/CLI/Python/Node）。
- 对外语义不变（行为等价，错误分类不扩张）。
- 每个任务独立 PR，且每 PR 必跑全门禁。

## 3. 审计映射表

| 断言ID | 原文摘录 | 事实证据文件 | 影响级别 | 风险 | 建议动作 | 映射任务ID | 状态 |
|---|---|---|---|---|---|---|---|
| A-001 | “Phase 0: 审计与护栏（前置）” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-Architecture.md:1150` | P0 | 若跳过基线将无法判定等价 | 先落地 R0 基线文档与护栏清单 | R0 | Done |
| A-002 | “Phase 1a: 纯文件拆分 + CLI 边界收敛” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-Architecture.md:1158` | P0 | 拆分与边界改造混改导致回归定位困难 | 拆为 R1/R2/R3/S1 四个独立任务 | R1,R2,R3,S1 | Done |
| A-003 | “query 三巨型文件需拆分” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:142`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:196`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/evaluator.rs:378` | P1 | 大文件继续增长将降低可维护性 | 先拆 API，再拆执行，再拆 evaluator | R1,R2,R3 | Done |
| A-004 | “CLI 边界收敛到门面层” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:2`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:248`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:212` | P0 | CLI 直连 storage 造成层级漂移 | 移除 CLI 对 storage 实现层直接依赖（含 vacuum 路径） | S1 | Done |
| A-005 | “每 PR 全门禁” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:38`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:42` | P0 | 门禁缺跑会放大语义漂移风险 | 固化为所有任务 DoD 硬条件 | R0,R1,R2,R3,S1,S2,S3 | Done |
| A-006 | “当前 Tier-3 仍未达 Beta 阈值” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:11`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:99` | P0 | 若在低通过率下混入语义变更，回归噪音过高 | 本轮重构限定“结构等价” | R0,R1,R2,R3 | Done |
| A-007 | “ReturnOrderBy2 仍有失败簇” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:103` | P1 | 语义修复与结构拆分混改会互相污染 | 将语义修复后置至 R4 独立任务 | BETA-03R4 | Open |
| A-008 | “Db 与 StorageSnapshot 的桥接层仍在” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:50`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:208` | P1 | 边界职责不清会导致重复改动 | 在 S1/S2 明确边界与调用方向 | S1,S2 | Done |
| A-009 | “门禁脚本具备 tier0-3 参数能力” | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/tck_tier_gate.sh:102` | P1 | 回归覆盖不足 | 固定 tier0-2 每 PR，tier3 作为阶段验证 | R0 | Done |

## 4. 任务文件索引

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/R0-baseline.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/R1-query-api-split.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/R2-executor-split.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/R3-evaluator-split.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/S1-cli-facade-boundary.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/S2-storage-readpath-boundary.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/S3-bindings-contract-regression.md`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/closure-report.md`

## 5. 执行矩阵（串行）

| 周次 | 阶段 | 任务 | 入口条件 | 出口条件 |
|---|---|---|---|---|
| Week 1 | Phase 0 | R0 | 本文档基线已建立 | 审计断言均可追溯到 `文件:行号` |
| Week 2 | Phase 1a | R1 | R0 完成 | query_api 拆分后全门禁通过 |
| Week 3 | Phase 1a | R2 -> R3 -> S1 | 前序任务全绿 | 三文件拆分 + CLI 边界收敛全绿 |
| Week 4 | Phase 1b/1c | 类型统一 + LogicalPlan | Phase 1a 稳定 | 等价回归通过 |
| Week 5 | Phase 2 | S2 -> S3 | Phase 1c 通过 | storage/bindings 边界任务闭环 |
| Week 6 | Phase 3 | closure-report | 所有任务完成 | 无 P0 回归，形成闭环报告 |

## 6. 最新推进（2026-02-13）

- Phase1b 已落地“类型单一来源”基线：`PropertyValue/DecodeError` 上移到 API，storage 改为 re-export 薄层。  
  证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/property.rs`
- Phase1b 已落地 `EdgeKey` 统一来源（snapshot 本地定义移除，改为 API 别名）。  
  证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/snapshot.rs`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/read_path_convert.rs`
- Phase1b 已完成 facade 导出补全与入口函数补全（`GraphStore`、`PAGE_SIZE`、`backup`、`bulkload`）。  
  证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs`
- Phase1c 已完成查询入口重定向：`prepare` 现统一走 `LogicalPlan -> Optimizer -> PhysicalPlan` 管线。  
  证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/prepare_entry.rs`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/plan/logical.rs`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/plan/optimizer.rs`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/plan/physical.rs`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/planner.rs`
- Phase2 已完成 S2（Storage 读路径边界治理）收口验证并转 `Done`。  
  证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/S2-storage-readpath-boundary.md`
- Phase2 已完成 S3（bindings 契约回归）并增强 Node runtime 合同断言。  
  证据：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/refactor/S3-bindings-contract-regression.md`、`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/contract_smoke.sh`
- PR #131 已完成 crash gate 修复并通过全部 CI。  
  证据：`https://github.com/LuQing-Studio/nervusdb/pull/131`
- 已完成回归门禁（本轮已执行）：`cargo fmt --all -- --check`、`cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`、`cargo check --workspace`、`bash scripts/workspace_quick_test.sh`、`bash scripts/tck_tier_gate.sh tier0`、`bash scripts/tck_tier_gate.sh tier1`、`bash scripts/tck_tier_gate.sh tier2`、`bash scripts/binding_smoke.sh`、`bash scripts/contract_smoke.sh`。

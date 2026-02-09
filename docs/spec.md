# NervusDB v2 — 产品规格（Spec v2.2, SQLite-Beta 收敛版）

> 这份 spec 是 v2 的工程宪法：目标不是“看起来完成”，而是以可重复门禁达到 Beta 发布线。

## 1. 项目定位

- **一句话使命**：做一个纯 Rust 的单机嵌入式 Property Graph 数据库，提供 SQLite 风格“打开路径即用”的体验。
- **核心路径**：打开 DB → 写入 → 查询（含流式）→ 崩溃恢复 → 跨语言一致行为。

## 2. 范围与发布策略（锁定）

- 范围：仅单机嵌入（Rust + CLI + Python + Node）。
- 不做：远程服务、分布式、迁移兼容承诺。
- 允许：在 Beta 收敛期进行破坏性变更；但必须显式版本化存储格式 epoch。

## 3. Beta 硬门槛（必须同时满足）

1. 官方全量 openCypher TCK 通过率 **≥95%**（Tier-3 全量口径）。
2. warnings 视为阻断（fmt/clippy/tests/bindings 链路）。
3. 冻结阶段连续 **7 天** 主 CI + nightly 稳定。

> 未满足任一项，即视为“尚未达到图数据库界 SQLite（Beta）”。

## 4. 存储兼容与错误模型

- 引入并强制校验 `storage_format_epoch`。
- epoch 不匹配时，统一返回 `StorageFormatMismatch`（Compatibility 语义）。
- 错误分类统一为：`Syntax / Execution / Storage / Compatibility`。

跨语言映射约束：
- Python：`NervusError/SyntaxError/ExecutionError/StorageError/CompatibilityError`
- Node：结构化错误 payload（`code/category/message`）

## 5. 质量与门禁矩阵

### 5.1 PR 阻塞门禁

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
3. `bash scripts/workspace_quick_test.sh`
4. `bash scripts/tck_tier_gate.sh tier0|tier1|tier2`
5. `bash scripts/binding_smoke.sh && bash scripts/contract_smoke.sh`

### 5.2 Nightly / Manual 门禁

1. Tier-3 全量 + 失败聚类 + 通过率报告（`scripts/tck_full_rate.sh`）
2. Beta 阈值 gate（`scripts/beta_gate.sh`，默认 95%）
3. benchmark / chaos / soak / fuzz

## 6. 执行节奏

- Phase A：TCK 功能线（先冲到 95%）
- Phase B：稳定冻结线（7 天稳定窗）
- Phase C：性能封板线（大规模 SLO）

## 7. 文档单一事实源

- 规范：`docs/spec.md`
- 任务：`docs/tasks.md`
- 路线图：`docs/ROADMAP_2.0.md`
- 完成定义：`docs/memos/DONE.md`

若四者冲突，以“**代码 + CI/Nightly 门禁结果 + tasks 当前状态**”为准，并立即修正文档。

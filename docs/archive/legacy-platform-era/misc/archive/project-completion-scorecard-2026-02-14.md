# NervusDB v2 整体完成度清单（Scorecard）

> 审计日期：**2026-02-14**
>
> 本文目标：把“到底完成到哪了、还差什么、为什么感觉做不完”变成 **可量化**、**可核对** 的清单。

---

## 0. 为什么会有“一直做不完”的感觉？

这是正常的工程现象，原因主要有 3 个：

1. **功能线（openCypher TCK）本身很大**：当你开始以 TCK 作为合同（tests as contract）时，很多“看起来能跑”的实现会在边界语义上暴露缺口，修复往往是“补洞 + 回归矩阵”，体感像在“一直加东西”。
2. **Beta 的定义里包含时间维度**：`docs/spec.md` 明确要求“连续 7 天稳定窗”。这类工作不是写代码就能立刻完成，需要持续跑满窗口。
3. **文档口径不一致会放大焦虑**：仓库里同时存在 `ROADMAP.md`（旧口径）和 `docs/ROADMAP_2.0.md`（SQLite-Beta 收敛口径）。目前应以 `docs/spec.md` + `docs/tasks.md` 为准（`docs/spec.md` 已声明）。

---

## 1. 审计口径（强烈建议你只盯住一个口径）

本文提供 3 个口径，你可以按目标选择：

### 1.1 口径 A：功能正确性（TCK）

- 问题：**“openCypher 行为是否正确？”**
- 判断：以 Tier-3 官方全量 TCK 为准。

### 1.2 口径 B：SQLite-Beta 发布门槛（Spec）

- 问题：**“是否达到 SQLite-Beta 发布线？”**
- 判断：以 `docs/spec.md` 的 Beta 硬门槛为准：
  1. Tier-3 pass_rate ≥ 95%
  2. warnings 视为阻断（fmt/clippy/tests/bindings）
  3. 连续 7 天主 CI + nightly 稳定

### 1.3 口径 C：任务板完成度（Tasks Board）

- 问题：**“任务板上还有多少没做？”**
- 判断：以 `docs/tasks.md` 的 `Status` 统计为准。

---

## 2. 当前硬指标快照（可复核）

### 2.1 TCK Tier-3（官方全量）

- 2026-02-14：`3897/3897 = 100.00%`（skipped 0，failed 0）
- 证据：
  - `artifacts/tck/tier3-rate-2026-02-14.json`
  - `artifacts/tck/beta-04-error-step-bridge-tier3-full-2026-02-14.log`

对比（上一日）：

- 2026-02-13：`3682 passed, 199 skipped, 16 failed`，pass_rate `94.48%`
- 证据：`artifacts/tck/tier3-rate-2026-02-13.json`

### 2.2 任务板统计（来自 `docs/tasks.md` 表格自动统计）

- 统计口径：解析 `docs/tasks.md` 顶部主表（93 行有效任务）
- 结果：
  - `Done`: 82
  - `WIP`: 10
  - `Plan`: 1
  - Done 比例：`82/93 = 88.17%`

> 说明：这不等于“发布完成度”，因为 WIP/Plan 里包含时间维度（稳定窗）和性能封板等大项。

### 2.3 Beta Gate（`docs/tasks.md` 的 Beta Gate 小表）

- `BETA-01` Done（`storage_format_epoch` 校验）
- `BETA-02` Done（Tier-3 通过率统计 + 95% 阈值 gate）
- `BETA-03` Done（TCK ≥95%，当前已到 100%）
- `BETA-04` WIP（连续 7 天稳定窗）
- `BETA-05` Plan（性能 SLO 封板）

---

## 3. 100 分制评分（给你一个“可解释”的分数）

> 评分不是事实本身，而是把“完成度”变成可讨论的量。  
> 我给出 **两种** 100 分制：一个偏发布（推荐），一个偏功能（辅助）。

### 3.1 评分表（推荐）：SQLite-Beta 发布完成度（总分 100）

| 模块 | 分值 | 当前得分 | 依据（可复核） |
| --- | ---: | ---: | --- |
| A. TCK Tier-3 正确性（≥95%） | 40 | 40 | `tier3-rate-2026-02-14.json` 显示 100% |
| B. PR 阻断门禁（fmt/clippy/tier0/1/2/binding/contract） | 20 | 18 | 门禁脚本已存在并频繁在证据日志中全绿；但“持续零警告”属于持续条件 |
| C. 稳定窗（连续 7 天） | 25 | 4 | 当前仅 2026-02-14 达标（上一日 94.48% 不达标），按 1/7 计入进度；且 nightly 统一统计仍在 WIP |
| D. 性能封板（SLO） | 15 | 3 | 已有 benchmark 基础（如 T162），但 `BETA-05` 仍是 Plan，未建立 SLO 阻断 |
| **合计** | **100** | **65** | 见上表 |

结论（口径 B）：

- **SQLite-Beta 发布完成度：约 65/100**
- 主要差距集中在：
  - `BETA-04`（稳定窗必须“跑满 7 天”）
  - `BETA-05`（性能 SLO 封板未开始）

### 3.2 评分表（辅助）：功能线完成度（总分 100）

| 模块 | 分值 | 当前得分 | 依据 |
| --- | ---: | ---: | --- |
| openCypher TCK Tier-3 | 70 | 70 | 2026-02-14 达到 100% |
| 语义硬化（R13/R14） | 30 | 26 | `BETA-03R13` Done；`BETA-03R14` WIP 但已完成 W1~W12、审计热点清零 |
| **合计** | **100** | **96** | 功能正确性已非常接近“完成” |

结论（口径 A）：

- **功能正确性：约 96/100**（接近封顶）
- 目前继续做的 R14 类工作本质是“把边界语义变成长期稳定”，避免回归。

---

## 4. 未完成清单（准确列出：10 个 WIP + 1 个 Plan）

> 来源：`docs/tasks.md` 的 `Status != Done` 行（共 11 项）。

### 4.1 Beta Gate（发布阻塞）

- `BETA-04`（WIP）：连续 7 天主 CI + nightly 稳定窗  
  - 当前状态：只有 2026-02-14 的 Tier-3 rate 达标；上一日 94.48% 不达标，因此稳定窗至少从 2026-02-14 开始计。
  - 完成定义（建议明确写到 tasks/脚本里）：连续 7 天满足
    - Tier-3 rate ≥ 95% 且 failed=0（已有数据结构）
    - 主 CI 全绿（需要接入统计）
    - nightly（benchmark/chaos/soak/fuzz）无阻断失败（需要统一统计口径）

- `BETA-05`（Plan）：大规模性能 SLO 封板  
  - 完成定义：把读/写/向量检索 P99 门槛落为可重复基准，并在 CI/Nightly 做阻断。

### 4.2 稳定性/质量护栏（会影响稳定窗）

- `I5-01`（WIP）：`cargo-fuzz` 分层接入（已 nightly，待接入稳定窗统计）
- `I5-02`（WIP）：Chaos IO 门禁（已 nightly，待接入稳定窗统计）
- `I5-03`（WIP）：24h soak（已 nightly，待接入稳定窗统计）

### 4.3 工程体验/产品化（非立刻阻塞，但会影响“可用/可交付”）

- `M5-01`（WIP）：Python + Node 可用性收敛（契约覆盖与示例）
- `M5-02`（WIP）：用户文档与支持矩阵对齐（Beta 口径补全）
- `M5-03`（WIP）：与 Neo4j/Memgraph 对标基准（绑定 SLO gate）
- `M5-04`（WIP）：并发读热点优化（收敛到 Beta P99 门槛）
- `M5-05`（WIP）：HNSW 参数调优与默认策略（收敛到 recall/latency 发布门槛）

### 4.4 语义硬化（“不再出 silent null/吞错”的收口项）

- `BETA-03R14`（WIP）：runtime 语义一致性收口  
  - 现状：W1~W12 已推进，并已落地 `scripts/runtime_guard_audit.sh`，且 executor 热点已清零（见最新审计日志）。
  - 为什么还没改 Done：缺一个明确的 exit checklist（建议补齐：审计脚本 + tier0/1/2 + targeted matrix 全绿，然后置 Done）。

---

## 5. 建议你如何“看见终点”

如果你的目标是 **SQLite-Beta 发布**，你可以只盯住 3 件事（按 `docs/spec.md`）：

1. Tier-3 rate：持续 ≥95%（已达到，并且现在是 100%）
2. warnings 阻断：持续零告警（已建立门禁，后续维持）
3. 7 天稳定窗：**需要时间**（这是“感觉做不完”的主因）

然后把下一阶段拆成明确的小票据：

- 稳定窗统一统计（把主 CI / nightly 的状态落成每日 JSON 或表格）
- `BETA-05` 性能基准与阻断（先建立“跑得出来”的基准，再谈优化）

---

## 6. 附：证据索引（便于你抽查）

- Tier-3 rate：
  - `artifacts/tck/tier3-rate-2026-02-13.json`
  - `artifacts/tck/tier3-rate-2026-02-14.json`
- runtime guard 审计：
  - `scripts/runtime_guard_audit.sh`
  - `artifacts/tck/beta-04-r14w11-runtime-guard-audit-2026-02-14.log`
  - `artifacts/tck/beta-04-r14w12-runtime-guard-hotspot-fix-2026-02-14.log`


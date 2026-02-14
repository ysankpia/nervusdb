# TB1 Implementation Plan: NervusDB「图数据库界 SQLite（Beta）」门禁收敛

## 1. Overview
在不引入分布式/远程服务的前提下，将 NervusDB 收敛到单机嵌入式 Beta 发布门槛：
1) openCypher TCK 全量通过率 ≥95%；
2) 零 warning 阻断；
3) 冻结阶段连续 7 天主 CI + nightly 稳定。

## 2. Requirements Analysis
### 2.1 Usage Scenarios
1. 用户通过 Rust/CLI/Python/Node 任一入口打开数据库并执行读写。
2. 存储格式破坏升级时，系统明确拒绝旧格式并给出统一兼容错误。
3. 团队可通过自动化脚本持续观察 TCK 全量通过率与稳定趋势。

### 2.2 Functional Requirements
- [ ] 增加并强制校验 `storage_format_epoch`（header 持久化）。
- [ ] 打开旧 epoch 返回 `StorageFormatMismatch`，并映射到跨语言 Compatibility 类别。
- [ ] 新增 TCK Tier-3 全量通过率统计脚本，生成 JSON + Markdown 报告。
- [ ] 新增 Beta gate：通过率阈值默认 95%，不达标阻断。

### 2.3 Performance Goals
- Tier-3 全量统计脚本可在 nightly 流程内稳定执行并产出工件。
- Beta gate 不增加主 PR 门禁时延（仅 manual/nightly 强阻断）。

## 3. Test Case Design
### 3.1 Unit Test Cases
- Header 中 epoch 被篡改后，`Pager::open` 应失败并返回格式兼容错误。
- Rust `Error` 转换能将格式兼容错误映射为 `Compatibility`。
- Python 错误分类把 `storage format mismatch` 映射到 `CompatibilityError`。
- Node 错误分类输出结构化 `code/category/message`。

### 3.2 Integration Test Cases
- 运行 `scripts/tck_full_rate.sh` 生成 `artifacts/tck/tier3-rate.json/.md`。
- 运行 `scripts/beta_gate.sh`，当通过率低于阈值时返回非零。

### 3.3 Exception Scenarios
- 日志缺失 summary 时，统计脚本应返回可读错误。
- TCK 执行失败但 `ALLOW_FAIL=1` 时仍需产出报告并保留真实退出码信息。

## 4. Design Scheme
### 4.1 Core Principles and Architecture Decisions
- 兼容校验放在 `Pager` 元信息读取路径，避免 WAL 协议额外复杂度。
- 统一错误语义从 storage 到 v2，再到 Python/Node 绑定逐层映射。
- TCK 通过率以官方全量 harness 输出为数据源，避免二次口径。

### 4.2 API Design
- Storage: `storage_format_epoch`（meta header 字段）
- Rust Error: `Compatibility(String)`
- Python Error: `CompatibilityError`
- Node Error payload: `{ code, category, message }`（JSON string）

## 5. Implementation Plan
### Step 1: 先写失败测试（Risk: High）
- `pager` epoch mismatch case
- Rust/Python/Node 错误分类 case
- TCK 通过率脚本解析 case（最小日志样例）

### Step 2: 最小实现（Risk: High）
- `storage_format_epoch` 落地与校验
- 错误映射新增 Compatibility
- TCK 全量统计与 beta gate 脚本

### Step 3: 回归与文档（Risk: Medium）
- fmt/clippy/tests + 脚本回归
- 更新 `spec/tasks/roadmap/done` 与归档说明

## 6. Technical Key Points
- Header 字段向后兼容策略：旧文件无 epoch（0）视为 mismatch 并拒绝打开。
- Node N-API 错误对象限制下，使用可机读 JSON reason 保留结构化语义。
- TCK 通过率计算使用 `passed/(passed+failed+skipped+pending+undefined)` 作为官方口径近似。

## 7. Validation Plan
### 7.1 Unit Tests
- `cargo test -p nervusdb-storage pager::tests::*`
- `cargo test -p nervusdb error::tests::*`
- `cargo test -p nervusdb-pyo3`
- `cargo test --manifest-path nervusdb-node/Cargo.toml`

### 7.2 Integration/End-to-End Tests
- `bash scripts/tck_full_rate.sh`
- `bash scripts/beta_gate.sh`

### 7.3 Boundaries and Exception Scenarios
- 本任务不直接承诺“当下达到 95%”；先交付可量化、可阻断门禁基础设施。

## 8. Risk Assessment
| Risk Description | Impact Level | Mitigation Measures |
|---|---|---|
| epoch 校验误伤现有测试数据 | High | 只对 header epoch 字段判断；新增单测覆盖创建/重开路径 |
| Node 错误结构与现有调用不兼容 | Medium | 保持抛错行为不变，仅增强 message 为结构化 JSON |
| TCK 日志格式变化导致统计脚本失效 | Medium | 增加兜底解析与明确失败提示 |

## 9. Out of Scope
- 不在本任务中完成 TCK 功能缺口的全部语义修复。
- 不在本任务中实现 7 天稳定窗自动判定器（先落地日报与产物）。

## 10. Future Extensions
- 增加稳定窗计数脚本（读取过去 7 天 workflow 结果自动判定）。
- 引入跨语言 golden fixtures，保证错误码/值语义长期一致。

# TM4 Implementation Plan: Tiered TCK Gates（M4-07 / M4-08）

## 1. Overview
将原先一次性全量 TCK 执行改为分层门禁：PR 阻塞层可持续绿，full 回归放入 nightly 并沉淀失败聚类。

## 2. Requirements Analysis
### 2.1 Usage Scenarios
1. 开发者在 PR 中快速得到 clauses/expressions 回归信号。
2. 维护者在 nightly 查看全量失败趋势并选择修复簇。

### 2.2 Functional Requirements
- [x] 提供 Tier-0/1/2/3 四层执行入口。
- [x] Tier-1/2 通过白名单文件控制覆盖集。
- [x] Tier-3 失败时仍产出日志与聚类报告。

### 2.3 Performance Goals
- PR 门禁总时长可控（Tier-0/1/2 为分钟级）。
- Tier-3 允许长时执行（nightly 非阻塞）。

## 3. Test Case Design
### 3.1 Unit Test Cases
- 输入：Tier 参数非法；期望：脚本返回 2 并输出 usage。
- 输入：白名单缺失；期望：脚本快速失败并给出缺失文件路径。

### 3.2 Integration Test Cases
- `bash scripts/tck_tier_gate.sh tier0|tier1|tier2` 本地可通过。
- `TCK_ALLOW_FAIL=1 ... tier3` 产出 `tier3-full.log` 和 cluster 报告。

### 3.3 Exception Scenarios
- TCK harness 失败时，Tier-3 在 allow-fail 模式仍能生成工件。

## 4. Design Scheme
### 4.1 Core Principles and Architecture Decisions
- `scripts/tck_tier_gate.sh` 为唯一入口，Makefile/CI 统一调用。
- 白名单文件位于 `scripts/tck_whitelist/`，避免硬编码 feature。
- 聚类脚本 `scripts/tck_failure_cluster.sh` 独立，支持离线复算。

### 4.2 API Design
- `scripts/tck_tier_gate.sh [tier0|tier1|tier2|tier3]`
- 环境变量：`TCK_ALLOW_FAIL`、`TCK_REPORT_DIR`

## 5. Implementation Plan
### Step 1: 分层脚本与白名单（Risk: High）
- 先补失败用例（缺 whitelist、未知 tier）
- 最小实现脚本与 whitelist
- 运行 tier0/1/2 回归

### Step 2: nightly 与工件（Risk: Medium）
- 接入 GitHub Actions nightly
- 固定上传 `artifacts/tck/`

## 6. Technical Key Points
- 日志解析应容忍无 summary block 的日志。
- 脚本必须在 `set -euo pipefail` 下稳定运行。

## 7. Validation Plan
### 7.1 Unit Tests
脚本参数/错误处理验证。

### 7.2 Integration/End-to-End Tests
- PR：Tier-0/1/2 全绿。
- Nightly：Tier-3 工件可下载并含聚类内容。

### 7.3 Boundaries and Exception Scenarios
- 仅对 gate 编排负责，不等同于“全量 TCK 全通过”。

## 8. Risk Assessment
| Risk Description | Impact Level | Mitigation Measures |
|---|---|---|
| 全量 TCK 失败导致主 CI 红 | High | Tier-3 改 nightly 非阻塞 |
| 白名单漂移导致误判 | Medium | 每次 PR 修改白名单必须附说明 |

## 9. Out of Scope
- 不在此任务内修完所有 TCK 失败簇。

## 10. Future Extensions
- 自动统计 whitelist 通过率趋势并写入 `docs/perf/`。

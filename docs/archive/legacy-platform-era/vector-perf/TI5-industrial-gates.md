# TI5 Implementation Plan: Industrial Quality Gates（Fuzz / Chaos / Soak）

## 1. Overview
为 Roadmap Phase 3 建立可执行的工业质量门禁骨架，并通过 nightly/manual workflow 持续沉淀稳定性信号。

## 2. Requirements Analysis
### 2.1 Usage Scenarios
1. 每周自动运行 fuzz/chaos/soak 并保存产物。
2. 失败时可快速定位到最小复现入口。

### 2.2 Functional Requirements
- [x] `cargo-fuzz` target scaffold + nightly workflow。
- [x] Chaos IO gate 脚本 + workflow。
- [x] Soak 稳压脚本 + workflow。

### 2.3 Performance Goals
- 不阻塞主 PR CI。
- Nightly 任务具备超时控制与产物上传。

## 3. Test Case Design
### 3.1 Unit Test Cases
- 脚本参数缺省时使用安全默认值。
- 故障注入路径返回非零时正确失败。

### 3.2 Integration Test Cases
- `scripts/chaos_io_gate.sh` 本地可运行。
- `scripts/soak_stability.sh` 可在缩短时长参数下完成。
- `fuzz/query_prepare` 可启动并跑固定时长。

### 3.3 Exception Scenarios
- 只读目录写入必须失败（permission denied）。

## 4. Design Scheme
### 4.1 Core Principles and Architecture Decisions
- 脚本位于 `scripts/`，workflow 只编排，不内嵌复杂逻辑。
- 非阻塞主线，但保留可审计产物。

### 4.2 API Design
- `bash scripts/chaos_io_gate.sh`
- `SOAK_MINUTES=<n> bash scripts/soak_stability.sh`
- `cd fuzz && cargo fuzz run query_prepare -- -max_total_time=300`

## 5. Implementation Plan
### Step 1: 工具脚本（Risk: High）
- 先补失败条件验证（权限错误）
- 加入默认参数和日志输出

### Step 2: workflow 编排（Risk: Medium）
- 周期触发 + 手动触发
- 上传 `artifacts/` 便于排障

## 6. Technical Key Points
- Chaos 脚本需覆盖 crash-test + 权限失败两类路径。
- Soak 默认参数需兼顾时长与可观察性。

## 7. Validation Plan
### 7.1 Unit Tests
脚本分支行为检查。

### 7.2 Integration/End-to-End Tests
nightly workflow 干跑与本地缩短参数回归。

### 7.3 Boundaries and Exception Scenarios
- 24h soak 的长期趋势分析不在本阶段完成。

## 8. Risk Assessment
| Risk Description | Impact Level | Mitigation Measures |
|---|---|---|
| Nightly 超时或资源不足 | Medium | 设定 timeout + 分任务执行 |
| 失败缺少可复现上下文 | High | 统一产物路径并保留日志 |

## 9. Out of Scope
- 不在此阶段建立自动缺陷分派系统。

## 10. Future Extensions
- 追加 parser/planner/executor 多目标 fuzz 与 corpus 管理。

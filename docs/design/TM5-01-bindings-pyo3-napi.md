# TM5-01 Implementation Plan: PyO3 + N-API Binding Surface

## 1. Overview
统一 Python/Node 的最小可用绑定能力，并确保错误类型、查询输出语义和事务入口一致。

## 2. Requirements Analysis
### 2.1 Usage Scenarios
1. Python 用户可直接 `Db.query()` 与 `Db.query_stream()`。
2. Node 用户可用 `open/query/begin_write/commit/rollback` 执行最小读写。
3. 多语言返回结果列名和值语义保持一致。

### 2.2 Functional Requirements
- [x] Python 异常分层：`NervusError/SyntaxError/ExecutionError/StorageError`。
- [x] Python 新增 `query_stream()`。
- [x] Node N-API scaffold + TS 类型定义。
- [x] 绑定 smoke 与跨语言 contract smoke 脚本。

### 2.3 Performance Goals
- `query_stream` 先保证 API 稳定，后续内部可演进为分块流式。

## 3. Test Case Design
### 3.1 Unit Test Cases
- 异常分类：syntax/storage/execution 归类正确。
- `QueryStream` 保序迭代，`len` 不变。

### 3.2 Integration Test Cases
- Python: `query/query_stream/close` 正常与异常路径。
- Node: scaffold `cargo check` 通过。
- `scripts/contract_smoke.sh` 在 PR 中可执行。

### 3.3 Exception Scenarios
- 已关闭 DB 再查询应落到 `StorageError`。
- 非法 Cypher 应落到 `SyntaxError`。

## 4. Design Scheme
### 4.1 Core Principles and Architecture Decisions
- Python 端以 `classify_nervus_error` 做统一映射。
- Node 端先提供稳定 API 面，再补运行时行为回归。

### 4.2 API Design
- Python: `Db.query(cypher, params?)`、`Db.query_stream(cypher, params?)`
- Node: `Db.open(path)`, `query`, `execute_write`, `begin_write`, `WriteTxn.query/commit/rollback`

## 5. Implementation Plan
### Step 1: Python 异常与 stream（Risk: High）
- 先写失败单测（分类 + stream）
- 最小实现后通过 `cargo test -p nervusdb-pyo3`

### Step 2: Node scaffold（Risk: High）
- 落地 N-API crate 与 `index.d.ts`
- 用 `cargo check --manifest-path nervusdb-node/Cargo.toml` 验证

### Step 3: 契约快测（Risk: Medium）
- 集成 `scripts/binding_smoke.sh` 与 `scripts/contract_smoke.sh`
- 接入主 CI Linux gate

## 6. Technical Key Points
- Python `WriteTxn` 生命周期由 `Py<Db>` 强引用与计数器保护。
- Node 当前事务对象为 staged query 模式，后续可替换为真实长事务句柄。

## 7. Validation Plan
### 7.1 Unit Tests
`nervusdb-pyo3` 内部单测覆盖分类与 stream。

### 7.2 Integration/End-to-End Tests
- `bash scripts/binding_smoke.sh`
- `bash scripts/contract_smoke.sh`

### 7.3 Boundaries and Exception Scenarios
- Node 运行时兼容矩阵（不同 Node 版本）暂不在本轮完成。

## 8. Risk Assessment
| Risk Description | Impact Level | Mitigation Measures |
|---|---|---|
| 跨语言语义漂移 | High | 固化 contract smoke 并纳入 PR gate |
| Python 错误归类误判 | Medium | 按关键错误模式补回归用例 |

## 9. Out of Scope
- 不在本任务中完成 Node 包发布（npm 发布链路）。

## 10. Future Extensions
- 增加 Rust/Python/Node 同查询 JSON golden fixtures。

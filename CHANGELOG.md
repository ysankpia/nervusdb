# 变更日志

> 主线 changelog 只记录当前架构（`redb` 单文件 + Rust Core + 薄绑定）的**可验证**变更。旧时代记录已归档：`_archive/CHANGELOG_legacy.md`。

## 未发布

- （占位）下一次改动从这里开始写。

## [1.0.0] - 2025-12-23

### 新增

- **v1.0 契约封版**：`nervusdb-core/include/nervusdb.h` 作为稳定 C ABI（SQLite 风格 stmt API）
- **Cypher 子集白名单**：新增 `LIMIT`；白名单外语法 fail-fast（返回 `not implemented: ...`）

### 变更

- **绑定层收敛**：Node 侧提供 Statement/流式消费路径，避免大结果集在 V8 堆里“对象爆炸”

### 验证

- **Crash Gate**：发布前复跑 1000 次 crash-test（门禁要求 0 失败）

## [0.1.0] - 2025-12-23

### 新增

- **单文件存储**：Triples / Dictionary / Properties 全部落在同一个 `.redb` 文件
- **稳定 C ABI（SQLite 风格）**：`nervusdb_open` / `prepare_v2 → step → column_* → finalize`（并保留 `exec_cypher(JSON)` 兼容）
- **Crash 门禁**：PR 跑 crash-smoke（5 次），nightly/tag 跑 crash-gate（1000 次）

### 变更

- **索引收敛**：从“六序索引”收敛为 `SPO / POS / OSP` 三索引（降低写放大）
- **Temporal 默认关闭**：Temporal 作为可选 Cargo feature `temporal`（Default OFF）
- **迁移策略**：不在 `open()` 自动迁移；提供 `nervus-migrate` 让用户显式运行

### 修复

- 修复极端 crash 场景下首轮建库被 `SIGKILL` 导致的 `redb invalid data`（确保存在 committed snapshot）
- 修复 Node 侧 Cypher 调用路径错误（返回错数据的致命 bug）

### 文档

- 性能报告与基准方法论刷新（修正 `redb(raw)` 基线，补齐 `exec_cypher` vs `stmt` 对比）

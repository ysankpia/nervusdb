# NervusDB v2 — 产品规格（Spec v2.1, 解冻执行版）

> 这份 spec 是 v2 的工程宪法：对齐“已实现事实 + 进行中任务 + 验收门禁”，避免文档与代码打架。

## 1. 项目定位

- **一句话使命**：做一个纯 Rust 的嵌入式 Property Graph 数据库，像 SQLite 一样“单路径打开即用”，并以图遍历为核心。
- **核心路径**：打开 DB → 写入 → 查询（含流式消费）→ 崩溃恢复可验证。

## 2. 范围声明（解冻）

- v2 继续保持：**新 crate / 新磁盘格式 / 不兼容 v1**。
- 与“冻结范围”不同，当前阶段进入 **全量 Roadmap 收尾执行**：
  - M4：Cypher 兼容门禁扩展（TCK clauses/expressions）
  - M5：绑定、文档、基准、并发优化、HNSW 调优
  - Industrial：Fuzz / Chaos / Soak
- 对外口径仍坚持：**通过自动化门禁才算支持**。

## 3. 当前事实（已实现）

- 存储：`.ndb + .wal`，页大小 `8KB`，支持 checkpoint/vacuum/backup。
- 并发：Single Writer + Snapshot Readers。
- 恢复：WAL replay + checkpoint/crash gate。
- 查询：已完成 T300~T331 与 M4-01~M4-11 的主体能力（详见 `docs/tasks.md`）。

## 4. 核心约束

- **安全**：不得硬编码密钥；崩溃一致性是硬门槛。
- **复杂度**：优先可读、可回滚、可测试的最小实现。
- **兼容**：不以“口头支持”替代验收；未通过门禁视为未支持。
- **发布**：主干必须可构建、可测试、可回归。

## 5. 技术路线（本轮锁定）

- **TCK 策略**：分层门禁（Tier-0/Tier-1/Tier-2 阻塞；Tier-3 nightly）。
- **绑定策略**：`PyO3 + N-API`，不迁移 UniFFI。
- **交付节奏**：分阶段串行推进，阶段内保持 CI 可绿。

## 6. 质量与门禁

### 6.1 PR 阻塞门禁

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
3. workspace 快速测试
4. TCK Tier-0/Tier-1/Tier-2
5. Python/Node smoke + 跨语言契约快测

### 6.2 Nightly / Manual 门禁

1. TCK Tier-3 全量回归 + 失败聚类报告
2. benchmark 对标（含 Neo4j/Memgraph）
3. chaos 测试
4. 24h soak 稳定性测试
5. fuzz 长跑

## 7. 文档与任务单一事实源

- 路线图：`docs/ROADMAP_2.0.md`
- 任务状态：`docs/tasks.md`
- 完成定义：`docs/memos/DONE.md`
- 对外能力矩阵：`docs/reference/cypher_support.md`

若四者冲突，以“**代码 + CI 门禁结果 + tasks 当前状态**”为准，并立即修正文档。

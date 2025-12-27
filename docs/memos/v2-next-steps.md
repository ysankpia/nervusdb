# 现状分析与下一步建议

## 1. 现状评估 (Status Assessment)

根据 `docs/task_progress.md` 和代码库现状，**v2.0.0-alpha1** 的目标已实质性达成：

- **核心内核 (v2-storage)**: Pager, WAL, LSM-style IDMap (`u64`->`u32`), Snapshot Isolation 已稳定。
- **查询引擎 (v2-query)**: 支持 M3 定义的最小 Cypher 子集 (单跳, CREATE, DELETE, WHERE)。
- **CLI**: `nervusdb v2 query` 验收路径已打通，支持流式 NDJSON 输出。
- **文档**: 规范 (`spec.md`) 和支持列表 (`cypher_support.md`) 已更新。

**结论**: 项目已具备发布 **v2.0.0-alpha1** 的条件。

## 2. 待确认项决策建议 (Pending Decisions)

针对 `spec.md` 第 7 节的遗留问题，基于代码现状 (`nervusdb-v2-storage/src/idmap.rs`, `nervusdb-v2-query`) 的分析如下：

### Q1: v2 对外 ID 类型 (External ID)
- **现状**: 代码硬编码为 `pub type ExternalId = u64;`。
- **建议**: **维持 `u64`**。
  - 理由：作为嵌入式高性能内核，`u64` 是最高效的。String ID 引入的 intern/hash 复杂度和存储开销不符合 "Core" 定位。若用户需要 String ID，应在应用层或更高的 binding 层 (如 Node.js binding) 处理映射，或者作为后续 v2.1 的 `feature` 引入。
- **行动**: 在 `spec.md` 中将此项标记为“已决策：仅支持 `u64`”。

### Q2: 公开入口 (Public Entry Point)
- **现状**: `nervusdb-v2` (Storage/Txn) 与 `nervusdb-v2-query` (Planner/Executor) 是分离的 crate。用户需要分别引入。
- **建议**: **保持分离，但增加 Facade 便捷方法**。
  - 理由：架构上解耦是对的。但为了 "SQLite 体验"，建议在 `nervusdb-v2` crate 中添加 `feature = "query"`，开启后在 `Db` 或 `ReadTxn` 上提供 `.query(cypher: &str)` 的快捷代理方法，重导出必要的 query 类型。
- **行动**: 作为一个 Low Risk 任务 (T58) 加入列表，提升 DX (Developer Experience)。

### Q3: Alpha1 发布口径 (Cypher Whitelist)
- **现状**: `nervusdb-v2-query/src/lib.rs` 和 `docs/reference/cypher_support.md` 高度一致。
- **建议**: **严格执行白名单**。
  - 列表：`RETURN 1`, 单跳 `MATCH`, `WHERE` (基本比较), `CREATE` (Node/Edge), `DELETE`, `DETACH DELETE`, `LIMIT`。
  - 任何超出此范围的语法（如 `OPTIONAL MATCH`, 多跳, 聚合）应直接报错 `NotImplemented`，而不是产生未定义行为。

## 3. 下一步计划 (Next Steps)

建议按以下顺序执行：

1.  **决策落地**: 更新 `spec.md` 第 7 节，记录上述决策。
2.  **版本发布**: Tag `v2.0.0-alpha1`。
3.  **DX 优化 (T58)**: 在 `nervusdb-v2` 中集成 Query Facade (可选，但推荐)。
4.  **属性存储完善 (T54)**: 这是目前唯一的 WIP 阻塞项 (Blocker)，需要集中精力完成属性在 WAL 和 MemTable 中的完整流转，确保 `WHERE` 子句能稳定工作。

请确认是否同意上述决策？同意后我将更新 `spec.md` 并创建 T58 任务。

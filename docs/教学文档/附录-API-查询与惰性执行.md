# 附录 · 查询与惰性执行 API

> 本文档总结查询相关的关键 API 与行为约定：惰性执行、流式收集、路径与诊断（explain/TRACE），以及 `withSnapshot` 的特殊语义与迁移建议。

## 总览

- `find(criteria, { anchor? })`：默认返回惰性链（LazyQueryBuilder）。
  - `withSnapshot(async (snap) => snap.find(...))` 回调内，为保证快照一致性，`find()` 回落为“即刻物化”。
- `collect()`：异步统一收集为数组（在惰性链上不产生大内存峰值）。
- `batch(size)`：按批次异步产出三元组数组（Lazy 上为真流式）。
- `variablePathStream(relation, { min?, max }, { direction?, uniqueness?, target? })`：真流式变长路径（层序 BFS）。
- `followPath(predicate, { min?, max }, { direction? })`：在 Lazy 上以 FOLLOW_PATH 计划真流式执行，在 EAGER 上同步物化。
- `explain()`：输出 LAZY/EAGER 计划摘要与估算信息。

## 惰性链（LazyQueryBuilder）与快照语义

- 默认 `find()` 返回 Lazy：链式构建期间不物化，执行阶段逐步产出。
- `withSnapshot` 回调内：`find()` 回落为“即刻物化”，确保整个链路在固定的 manifest epoch 上执行。
- 读取大结果时请优先使用 `for await ... of q.batch(n)` 或 `await q.collect()`，避免 `all()` 造成内存峰值。

## 结构化过滤（推荐）

- `whereProperty(name, op, value, target='node'|'edge')`：
  - `target='edge'` 仅支持 `op='='` 等值过滤；
  - `target='node'` 支持 `=, >, >=, <, <=`；
- `whereLabel(labels, { on='both'|'subject'|'object', mode='AND'|'OR' })`：
  - 用于主体/客体节点标签过滤；
- 迁移建议：避免 `where(predicate)` 在内存中过滤大结果集。请优先使用 `whereProperty/whereLabel`，或在 `follow` 阶段通过属性索引下推过滤。

## 变长路径

- `variablePath(relation, { min?, max, direction?, uniqueness? })`：同步物化版（保持历史 API）。
- `variablePathStream(relation, { min?, max }, { direction?, uniqueness?, target? })`：真流式 BFS，满足跳数范围与唯一性约束，逐条产出路径结果。
- `followPath(predicate, { min?, max }, { direction? })`：
  - Lazy：使用 FOLLOW_PATH 计划在执行期流式层序扩展，depth ∈ [min..max] 时产出“最后一跳”边；
  - EAGER：保持旧实现的同步物化。

## 诊断与估算（explain/TRACE）

- `explain()` 返回：
  - `type`: 'LAZY' | 'EAGER'
  - `plan`: 计划节点摘要（LAZY）
  - `estimate`: 估算摘要：
    - `order`: 选用的六序索引（FIND 阶段）
    - `upperBound`: 基于主维度页覆盖的上界（保守）
    - `pagesForPrimary`: 主维度定值时命中的页数
    - `hotnessPrimary`: 主维度（值）热度合并计数（对偶顺序合并）
    - `estimatedOutput`: 传播后的粗略输出量（考虑 FOLLOW 倍增、skip/limit、union）
    - `stages`: 执行阶段估算的记录，例如 `{ type: 'FOLLOW', factor, output, order }`
- `SYNAPSEDB_TRACE_QUERY=1`：执行期输出各 stage 的计数与耗时（仅 Lazy）。

## 环境变量

- `SYNAPSEDB_QUERY_WARN_THRESHOLD`：内存物化与同步遍历的警告阈值（默认 5000；0 关闭）。
- `SYNAPSEDB_SILENCE_QUERY_WARNINGS=1`：关闭查询警告。
- `SYNAPSEDB_TRACE_QUERY=1`：启用 Lazy 阶段的 TRACE 日志。

## 注意与最佳实践

- 面对大图：优先链上使用 `whereProperty/whereLabel` 下推过滤；配合 `batch(n)` 逐步处理。
- 路径：大范围 `max` 应配合唯一性与去重；必要时加 `limit/skip` 控制外部消费速率。
- 估算：`upperBound/estimatedOutput` 为方向性指标，重在量级与组合的单调性；不保证精确值。

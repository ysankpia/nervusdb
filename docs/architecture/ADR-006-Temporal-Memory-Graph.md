# ADR-006: Temporal Memory Graph Query API

## 背景

随着时间记忆管线成为 NervusDB 的默认能力，我们需要为客户端提供清晰的时间线查询入口，让开发者可以检索实体的时间事实、追踪事件来源，并在将来下沉到原生核心时保持 API 一致性。

## 决策

- 在 TypeScript 层新增 `TemporalTimelineBuilder`，作为 `NervusDB.memory.timelineBuilder(entityId)` 的返回值，提供链式时间过滤能力。
- `timelineBuilder` 直接调用现有 `PersistentStore.queryTemporalTimeline`/`traceTemporalFact`，避免额外复制。
- 保留原有 `memory.timeline()` 同步 API 以兼容早期代码。
- 记录 Rust 核心尚未支持 `as_of`/`between` 的事实，后续版本会在 `nervusdb_core` 中实现同等语义。
- 自 v0.5.0 起，`NervusDB.open()` 默认初始化 `TemporalMemoryStore`（生成 `<db>.temporal.json`），`db.memory` API 无需额外 wiring 即可使用。

## 后果

- 正面：查询 DSL 得到直观的时间入口；测试覆盖 `predicate`、`role`、`asOf`、`between` 以及溯源链路。
- 负面：当前实现仍依赖 TypeScript JSON 存储，Rust 核心时间过滤尚未就绪。缓解措施：在 ADR 中明确 TODO，并通过统一 builder API 保留未来迁移空间。

## 变更记录

- 2025-11-06：初始决策，发布时间线查询构建器并补充单元测试。
- 2025-11-06：时间记忆默认集成，补充 README、Release Notes 及冒烟测试。

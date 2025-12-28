// =======================
// NervusDB - 嵌入式图数据库 (Node.js Binding)
// =======================
// 架构说明：这是 Rust Core 的薄包装层
// 所有复杂逻辑（算法、索引、解析器）都在 nervusdb-core 实现

// =======================
// 核心层导出
// =======================
export * as Core from './core/index.js';

// =======================
// 主入口
// =======================
export { NervusDB } from './nervusDb.js';
export type {
  FactRecord,
  FactInput,
  CypherRecord,
  CypherResult,
  CypherExecutionOptions,
  GraphAlgorithmsAPI,
  NativePageRankEntry,
  NativePageRankResult,
  NativePathResult,
  TemporalMemoryAPI,
  TemporalEpisodeInput,
  TemporalEpisodeLinkRecord,
  TemporalEnsureEntityOptions,
  TemporalFactWriteInput,
  TemporalStoredEpisode,
  TemporalStoredEntity,
  TemporalStoredFact,
  TemporalTimelineQuery,
} from './nervusDb.js';

// 向后兼容别名
export { NervusDB as CoreNervusDB } from './nervusDb.js';
export { NervusDB as ExtendedNervusDB } from './nervusDb.js';

// =======================
// 存储层
// =======================
export { PersistentStore } from './core/storage/persistentStore.js';
export type { PersistedFact } from './core/storage/persistentStore.js';

// 时间记忆（通过 Native 调用 Rust）
export { TemporalMemoryStore } from './core/storage/temporal/temporalStore.js';

// =======================
// 配置
// =======================
export type {
  NervusDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';

// =======================
// 已移除的功能（v2.0）
// =======================
// 以下功能已从 TypeScript 移除，将在 Rust Core 重新实现：
// - 图算法（PageRank, Dijkstra, Louvain）
// - 全文检索（TF-IDF, BM25）
// - 空间索引（R-Tree）
// - QueryBuilder, LazyQueryBuilder -> 使用 cypher() 方法
// - TypedNervusDB, TypedQueryBuilder -> 使用 cypher() 方法

// =======================
// 分层导出（支持 Tree-shaking）
// =======================

// 核心层：数据库内核（对标 Rust 项目）
export * as Core from './core/index.js';

// 扩展层：应用层功能（TypeScript 独有）
export * as Extensions from './extensions/index.js';

// 记忆层：时间感知记忆组件
export { TemporalMemoryStore } from './core/storage/temporal/temporalStore.js';
export { TemporalMemoryIngestor } from './memory/temporal/ingestor.js';
export { TemporalTimelineBuilder } from './memory/temporal/timelineBuilder.js';

// =======================
// 核心导出
// =======================

export { NervusDB } from './synapseDb.js';
export type {
  FactRecord,
  FactInput,
  TemporalMemoryAPI,
  TemporalEpisodeInput,
  TemporalEpisodeLinkRecord,
  TemporalEnsureEntityOptions,
  TemporalFactWriteInput,
  TemporalStoredEpisode,
  TemporalStoredEntity,
  TemporalStoredFact,
  TemporalTimelineQuery,
  TemporalMessageInput,
  TemporalConversationContext,
} from './synapseDb.js';

// 向后兼容别名（保留旧 API）
export { NervusDB as CoreNervusDB } from './synapseDb.js';
export { NervusDB as ExtendedNervusDB } from './synapseDb.js';

// 插件接口与管理器
export type { NervusDBPlugin } from './plugins/base.js';
export { PluginManager } from './plugins/base.js';

// 内置插件
export { PathfindingPlugin } from './plugins/pathfinding.js';
/** @experimental Cypher 查询语言仍处于实验阶段 */
export { CypherPlugin } from './plugins/cypher.js';
export { AggregationPlugin } from './plugins/aggregation.js';

// =======================
// 存储与查询
// =======================

export { PersistentStore } from './core/storage/persistentStore.js';
export type { PersistedFact } from './core/storage/persistentStore.js';
export { QueryBuilder } from './core/query/queryBuilder.js';
export { LazyQueryBuilder } from './core/query/queryBuilder.js';
export type {
  FactCriteria,
  FrontierOrientation,
  PropertyFilter,
} from './core/query/queryBuilder.js';
export { AggregationPipeline } from './extensions/query/aggregation.js';

// =======================
// 配置与选项
// =======================

export type {
  NervusDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';

// 新增：类型增强版本
export {
  TypedNervusDBFactory as TypedNervusDB,
  TypeSafeQueries,
  TypedQueryBuilderImpl,
} from './typedNervusDb.js';
export type {
  TypedNervusDB as TypedDB,
  TypedQueryBuilder,
  TypedFactInput,
  TypedFactOptions,
  TypedFactRecord,
  TypedPropertyFilter,
  NodeProperties,
  EdgeProperties,
  Labels,
  TypedNodeProperties,
  InferQueryResult,
  // 预定义类型
  PersonNode,
  RelationshipEdge,
  EntityNode,
  KnowledgeEdge,
  CodeNode,
  DependencyEdge,
} from './types/enhanced.js';

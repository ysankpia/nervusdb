// =======================
// 核心导出
// =======================

export { SynapseDB } from './synapseDb.js';
export type { FactRecord, FactInput } from './synapseDb.js';

// 向后兼容别名（保留旧 API）
export { SynapseDB as CoreSynapseDB } from './synapseDb.js';
export { SynapseDB as ExtendedSynapseDB } from './synapseDb.js';

// 插件接口与管理器
export type { SynapseDBPlugin } from './plugins/base.js';
export { PluginManager } from './plugins/base.js';

// 内置插件
export { PathfindingPlugin } from './plugins/pathfinding.js';
/** @experimental Cypher 查询语言仍处于实验阶段 */
export { CypherPlugin } from './plugins/cypher.js';
export { AggregationPlugin } from './plugins/aggregation.js';

// =======================
// 存储与查询
// =======================

export { PersistentStore } from './storage/persistentStore.js';
export type { PersistedFact } from './storage/persistentStore.js';
export { QueryBuilder } from './query/queryBuilder.js';
export { LazyQueryBuilder } from './query/queryBuilder.js';
export type { FactCriteria, FrontierOrientation, PropertyFilter } from './query/queryBuilder.js';
export { AggregationPipeline } from './query/aggregation.js';

// =======================
// 配置与选项
// =======================

export type {
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';

// 新增：类型增强版本
export {
  TypedSynapseDBFactory as TypedSynapseDB,
  TypeSafeQueries,
  TypedQueryBuilderImpl,
} from './typedSynapseDb.js';
export type {
  TypedSynapseDB as TypedDB,
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

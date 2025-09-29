import { ExtendedSynapseDB, type SynapseDBPlugin } from './plugins/base.js';
import { PathfindingPlugin } from './plugins/pathfinding.js';
import { CypherPlugin } from './plugins/cypher.js';
import { AggregationPlugin } from './plugins/aggregation.js';
import { warnExperimental } from './utils/experimental.js';
import { TripleKey } from './storage/propertyStore.js';
import { PersistentStore } from './storage/persistentStore.js';
import {
  FactCriteria,
  FrontierOrientation,
  QueryBuilder,
  StreamingQueryBuilder,
  buildStreamingFindContext,
  buildFindContextFromProperty,
  buildFindContextFromLabel,
  PropertyFilter,
} from './query/queryBuilder.js';
import type {
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';
import { PatternBuilder } from './query/pattern/match.js';
import { AggregationPipeline } from './query/aggregation.js';
import type { CypherResult, CypherExecutionOptions } from './query/cypher.js';

export interface FactOptions {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

// 重新导出核心类型
export type { FactInput, FactRecord } from './storage/persistentStore.js';

/**
 * SynapseDB - 嵌入式三元组知识库（兼容性版本）
 *
 * 这是一个向后兼容的版本，包含所有插件功能。
 * 新项目推荐使用 CoreSynapseDB + 选择性插件的方式。
 *
 * "好品味"原则：使用组合而非继承，没有特殊情况。
 *
 * @example
 * ```typescript
 * const db = await SynapseDB.open('/path/to/database.synapsedb', {
 *   pageSize: 2000,
 *   enableLock: true,
 *   compression: { codec: 'brotli', level: 6 }
 * });
 *
 * db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
 * await db.flush();
 *
 * const results = db.find({ predicate: 'knows' }).all();
 * await db.close();
 * ```
 */
export class SynapseDB {
  private snapshotDepth = 0;

  private constructor(
    private readonly core: ExtendedSynapseDB,
    private readonly hasCypherPlugin: boolean,
  ) {}

  /**
   * 打开或创建 SynapseDB 数据库（包含所有插件）
   */
  static async open(path: string, options?: SynapseDBOpenOptions): Promise<SynapseDB> {
    const experimental = options?.experimental ?? {};
    const envEnableExperimental = process.env.SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES === '1';
    const enableCypher = experimental.cypher ?? envEnableExperimental;

    const plugins: SynapseDBPlugin[] = [new PathfindingPlugin(), new AggregationPlugin()];
    if (enableCypher) {
      plugins.push(new CypherPlugin());
      warnExperimental('Cypher 查询语言前端');
    }

    const extendedDb = await ExtendedSynapseDB.open(path, {
      ...(options ?? {}),
      plugins,
    });

    return new SynapseDB(extendedDb, enableCypher);
  }

  // ===================
  // 核心API委托（直接委托）
  // ===================

  addFact(
    fact: import('./storage/persistentStore.js').FactInput,
    options: FactOptions = {},
  ): import('./storage/persistentStore.js').FactRecord {
    return this.core.addFact(fact, options);
  }

  listFacts(): import('./storage/persistentStore.js').FactRecord[] {
    return this.core.listFacts();
  }

  // 流式查询：逐批返回事实记录，避免大结果集内存压力
  async *streamFacts(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    batchSize = 1000,
  ): AsyncGenerator<import('./storage/persistentStore.js').FactRecord[], void, unknown> {
    // 将字符串条件转换为ID条件
    const encodedCriteria: Partial<{ subjectId: number; predicateId: number; objectId: number }> =
      {};

    if (criteria?.subject) {
      const subjectId = this.core.getNodeId(criteria.subject);
      if (subjectId !== undefined) encodedCriteria.subjectId = subjectId;
      else return; // 主语不存在，返回空
    }

    if (criteria?.predicate) {
      const predicateId = this.core.getNodeId(criteria.predicate);
      if (predicateId !== undefined) encodedCriteria.predicateId = predicateId;
      else return; // 谓语不存在，返回空
    }

    if (criteria?.object) {
      const objectId = this.core.getNodeId(criteria.object);
      if (objectId !== undefined) encodedCriteria.objectId = objectId;
      else return; // 宾语不存在，返回空
    }

    // 使用底层流式查询
    const store = this.core.getStore();
    for await (const batch of store.streamFactRecords(encodedCriteria, batchSize)) {
      if (batch.length > 0) {
        yield batch;
      }
    }
  }

  // 兼容别名：满足测试与直觉 API（与 streamFacts 等价）
  findStream(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    options?: { batchSize?: number },
  ): AsyncIterable<import('./storage/persistentStore.js').FactRecord[]> {
    return this.streamFacts(criteria, options?.batchSize);
  }

  getNodeId(value: string): number | undefined {
    return this.core.getNodeId(value);
  }

  getNodeValue(id: number): string | undefined {
    return this.core.getNodeValue(id);
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | null {
    return this.core.getNodeProperties(nodeId);
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | null {
    return this.core.getEdgeProperties(key);
  }

  async flush(): Promise<void> {
    await this.core.flush();
  }

  find(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder {
    return this.core.find(criteria, options);
  }

  /**
   * 流式查询 - 真正内存高效的大数据集查询
   */
  async findStreaming(
    criteria: FactCriteria,
    options?: { anchor?: FrontierOrientation },
  ): Promise<StreamingQueryBuilder> {
    const anchor = options?.anchor ?? this.inferAnchor(criteria);
    const store = this.core.getStore();
    const pinned = store.getCurrentEpoch();

    // 流式查询始终使用快照模式以保证一致性
    try {
      await store.pushPinnedEpoch(pinned);
      const context = await buildStreamingFindContext(store, criteria, anchor);
      return new StreamingQueryBuilder(store, context, pinned);
    } finally {
      await store.popPinnedEpoch();
    }
  }

  /**
   * 基于节点属性进行查询
   */
  findByNodeProperty(
    propertyFilter: PropertyFilter,
    options?: { anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const store = this.core.getStore();
    const pinned = store.getCurrentEpoch();
    const hasPagedData = store.hasPagedIndexData();

    if (hasPagedData) {
      const pushPromise = store.pushPinnedEpoch(pinned);
      void pushPromise.catch(() => undefined);
      try {
        const context = buildFindContextFromProperty(store, propertyFilter, anchor, 'node');
        return QueryBuilder.fromFindResult(store, context, pinned);
      } finally {
        const popPromise = store.popPinnedEpoch();
        void popPromise.catch(() => undefined);
      }
    }

    const context = buildFindContextFromProperty(store, propertyFilter, anchor, 'node');
    return QueryBuilder.fromFindResult(store, context);
  }

  /**
   * 基于边属性进行查询
   */
  findByEdgeProperty(
    propertyFilter: PropertyFilter,
    options?: { anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const store = this.core.getStore();
    const pinned = store.getCurrentEpoch();
    const hasPagedData = store.hasPagedIndexData();

    if (hasPagedData) {
      const pushPromise = store.pushPinnedEpoch(pinned);
      void pushPromise.catch(() => undefined);
      try {
        const context = buildFindContextFromProperty(store, propertyFilter, anchor, 'edge');
        return QueryBuilder.fromFindResult(store, context, pinned);
      } finally {
        const popPromise = store.popPinnedEpoch();
        void popPromise.catch(() => undefined);
      }
    }

    const context = buildFindContextFromProperty(store, propertyFilter, anchor, 'edge');
    return QueryBuilder.fromFindResult(store, context);
  }

  /**
   * 基于节点标签进行查询
   */
  findByLabel(
    labels: string | string[],
    options?: { mode?: 'AND' | 'OR'; anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const store = this.core.getStore();
    const pinned = store.getCurrentEpoch();
    const hasPagedData = store.hasPagedIndexData();

    if (hasPagedData) {
      const pushPromise = store.pushPinnedEpoch(pinned);
      void pushPromise.catch(() => undefined);
      try {
        const context = buildFindContextFromLabel(store, labels, { mode: options?.mode }, anchor);
        return QueryBuilder.fromFindResult(store, context, pinned);
      } finally {
        const popPromise = store.popPinnedEpoch();
        void popPromise.catch(() => undefined);
      }
    }

    const context = buildFindContextFromLabel(store, labels, { mode: options?.mode }, anchor);
    return QueryBuilder.fromFindResult(store, context);
  }

  deleteFact(fact: import('./storage/persistentStore.js').FactInput): void {
    this.core.deleteFact(fact);
  }

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    this.core.setNodeProperties(nodeId, properties);
  }

  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    this.core.setEdgeProperties(key, properties);
  }

  // 事务批次控制（可选）：允许将多次写入合并为一次提交
  beginBatch(options?: BeginBatchOptions): void {
    this.core.beginBatch(options);
  }

  commitBatch(options?: CommitBatchOptions): void {
    this.core.commitBatch(options);
  }

  abortBatch(): void {
    this.core.abortBatch();
  }

  async close(): Promise<void> {
    await this.core.close();
  }

  // 读快照：在给定回调期间固定当前 epoch，避免 mid-chain 刷新 readers 造成视图漂移
  async withSnapshot<T>(fn: (db: SynapseDB) => Promise<T> | T): Promise<T> {
    const store = this.core.getStore();
    const epoch = store.getCurrentEpoch();
    try {
      this.snapshotDepth++;
      // 等待读者注册完成，确保快照安全
      await store.pushPinnedEpoch(epoch);
      return await fn(this);
    } finally {
      await store.popPinnedEpoch();
      this.snapshotDepth = Math.max(0, this.snapshotDepth - 1);
    }
  }

  // 暂存层指标（实验性）：仅用于观测与基准
  getStagingMetrics(): { lsmMemtable: number } {
    const store = this.core.getStore();
    return store.getStagingMetrics();
  }

  /**
   * 获取底层存储（用于高级功能如 Gremlin）
   */
  getStore(): PersistentStore {
    return this.core.getStore();
  }

  // ===================
  // 插件功能委托
  // ===================

  // 聚合入口
  aggregate(): AggregationPipeline {
    const plugin = this.core.plugin<AggregationPlugin>('aggregation');
    if (!plugin) throw new Error('Aggregation plugin not available');
    return plugin.aggregate();
  }

  // 模式匹配入口（最小实现）
  match(): PatternBuilder {
    return new PatternBuilder(this.core.getStore());
  }

  // 最短路径：基于 BFS，返回边序列（不存在则返回 null）
  shortestPath(
    from: string,
    to: string,
    options?: {
      predicates?: string[];
      maxHops?: number;
      direction?: 'forward' | 'reverse' | 'both';
    },
  ): import('./storage/persistentStore.js').FactRecord[] | null {
    const plugin = this.core.plugin<PathfindingPlugin>('pathfinding');
    if (!plugin) throw new Error('Pathfinding plugin not available');
    return plugin.shortestPath(from, to, options);
  }

  // 双向 BFS 最短路径（无权），对大图更高效 - 优化版本
  shortestPathBidirectional(
    from: string,
    to: string,
    options?: {
      predicates?: string[];
      maxHops?: number;
    },
  ): import('./storage/persistentStore.js').FactRecord[] | null {
    const plugin = this.core.plugin<PathfindingPlugin>('pathfinding');
    if (!plugin) throw new Error('Pathfinding plugin not available');
    return plugin.shortestPathBidirectional(from, to, options);
  }

  // Dijkstra 加权最短路径（权重来自边属性，默认字段 'weight'，缺省视为1）
  shortestPathWeighted(
    from: string,
    to: string,
    options?: { predicate?: string; weightProperty?: string },
  ): import('./storage/persistentStore.js').FactRecord[] | null {
    const plugin = this.core.plugin<PathfindingPlugin>('pathfinding');
    if (!plugin) throw new Error('Pathfinding plugin not available');
    return plugin.shortestPathWeighted(from, to, options);
  }

  // Cypher 极简子集：仅支持 MATCH (a)-[:REL]->(b) RETURN a,b
  cypher(query: string): Array<Record<string, unknown>> {
    return this.requireCypherPlugin().cypherSimple(query);
  }

  // ------------------
  // Cypher 异步标准接口
  // ------------------

  /**
   * 执行 Cypher 查询（标准异步接口）
   * 注意：为保持向后兼容，保留了上方同步版 `cypher()`（极简子集）。
   */
  async cypherQuery(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    return this.requireCypherPlugin().cypherQuery(statement, parameters, options);
  }

  /**
   * 执行只读 Cypher 查询
   */
  async cypherRead(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    return this.requireCypherPlugin().cypherRead(statement, parameters, options);
  }

  /**
   * 验证 Cypher 语法
   */
  validateCypher(statement: string): { valid: boolean; errors: string[] } {
    return this.requireCypherPlugin().validateCypher(statement);
  }

  /** 清理 Cypher 优化器缓存 */
  clearCypherOptimizationCache(): void {
    this.requireCypherPlugin().clearCypherOptimizationCache();
  }

  /** 获取 Cypher 优化器统计信息 */
  getCypherOptimizerStats(): unknown {
    return this.requireCypherPlugin().getCypherOptimizerStats();
  }

  /** 预热 Cypher 优化器 */
  async warmUpCypherOptimizer(): Promise<void> {
    await this.requireCypherPlugin().warmUpCypherOptimizer();
  }

  // ===================
  // 私有方法
  // ===================

  private requireCypherPlugin(): CypherPlugin {
    if (!this.hasCypherPlugin) {
      throw new Error(
        'Cypher 插件未启用。请在 open() 时传入 experimental.cypher = true，或设置环境变量 SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES=1。',
      );
    }
    const plugin = this.core.plugin<CypherPlugin>('cypher');
    if (!plugin) {
      throw new Error('Cypher 插件加载失败，请检查 experimental 配置。');
    }
    return plugin;
  }

  private inferAnchor(criteria: FactCriteria): FrontierOrientation {
    const hasSubject = criteria.subject !== undefined;
    const hasObject = criteria.object !== undefined;
    const hasPredicate = criteria.predicate !== undefined;

    if (hasSubject && hasObject) {
      return 'both';
    }
    if (hasSubject) {
      return 'subject';
    }
    // p+o 查询通常希望锚定主语集合，便于后续正向联想
    if (hasObject && hasPredicate) {
      return 'subject';
    }
    // 仅 object 的场景保持锚定到宾语，便于 reverse follow（测试依赖）
    if (hasObject) {
      return 'object';
    }
    return 'object';
  }
}

export type {
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
  PropertyFilter,
  FrontierOrientation,
};

import { PluginManager, type SynapseDBPlugin } from './plugins/base.js';
import { PathfindingPlugin } from './plugins/pathfinding.js';
import { CypherPlugin } from './plugins/cypher.js';
import { AggregationPlugin } from './plugins/aggregation.js';
import { warnExperimental } from './utils/experimental.js';
import { TripleKey } from './storage/propertyStore.js';
import { PersistentStore, FactInput, FactRecord } from './storage/persistentStore.js';
import {
  FactCriteria,
  FrontierOrientation,
  QueryBuilder,
  StreamingQueryBuilder,
  buildFindContext,
  buildStreamingFindContext,
  buildFindContextFromProperty,
  buildFindContextFromLabel,
  PropertyFilter,
  LazyQueryBuilder,
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
export type { FactInput, FactRecord };

/**
 * SynapseDB - 嵌入式三元组知识库
 *
 * 统一的知识库实现，包含：
 * - 核心存储与查询功能
 * - 插件系统（默认加载 PathfindingPlugin + AggregationPlugin）
 * - 可选实验性功能（Cypher 查询）
 *
 * "好品味"原则：简单的 API，强大的功能，没有特殊情况。
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
  private pluginManager: PluginManager;
  private hasCypherPlugin: boolean;

  private constructor(
    private readonly store: PersistentStore,
    plugins: SynapseDBPlugin[],
    hasCypher: boolean,
  ) {
    this.hasCypherPlugin = hasCypher;
    this.pluginManager = new PluginManager(this, store);

    // 注册所有插件
    for (const plugin of plugins) {
      this.pluginManager.register(plugin);
    }
  }

  /**
   * 打开或创建 SynapseDB 数据库
   */
  static async open(path: string, options?: SynapseDBOpenOptions): Promise<SynapseDB> {
    const experimental = options?.experimental ?? {};
    const envEnableExperimental = process.env.SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES === '1';
    const enableCypher = experimental.cypher ?? envEnableExperimental;

    // 默认插件：Pathfinding + Aggregation
    const plugins: SynapseDBPlugin[] = [new PathfindingPlugin(), new AggregationPlugin()];

    // 可选实验性插件：Cypher
    if (enableCypher) {
      plugins.push(new CypherPlugin());
      warnExperimental('Cypher 查询语言前端');
    }

    // 打开存储
    const store = await PersistentStore.open(path, options ?? {});

    // 创建数据库实例
    const db = new SynapseDB(store, plugins, enableCypher);

    // 初始化插件
    await db.pluginManager.initialize();

    return db;
  }

  // ===================
  // 核心 API：存储与查询
  // ===================

  /**
   * 添加事实（三元组）
   */
  addFact(fact: FactInput, options: FactOptions = {}): FactRecord {
    const persisted = this.store.addFact(fact);

    if (options.subjectProperties) {
      this.store.setNodeProperties(persisted.subjectId, options.subjectProperties);
    }

    if (options.objectProperties) {
      this.store.setNodeProperties(persisted.objectId, options.objectProperties);
    }

    if (options.edgeProperties) {
      const tripleKey: TripleKey = {
        subjectId: persisted.subjectId,
        predicateId: persisted.predicateId,
        objectId: persisted.objectId,
      };
      this.store.setEdgeProperties(tripleKey, options.edgeProperties);
    }

    return {
      ...persisted,
      subjectProperties: this.store.getNodeProperties(persisted.subjectId),
      objectProperties: this.store.getNodeProperties(persisted.objectId),
      edgeProperties: this.store.getEdgeProperties({
        subjectId: persisted.subjectId,
        predicateId: persisted.predicateId,
        objectId: persisted.objectId,
      }),
    };
  }

  /**
   * 列出所有事实
   */
  listFacts(): FactRecord[] {
    return this.store.listFacts();
  }

  /**
   * 流式查询：逐批返回事实记录，避免大结果集内存压力
   */
  async *streamFacts(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    batchSize = 1000,
  ): AsyncGenerator<FactRecord[], void, unknown> {
    // 将字符串条件转换为ID条件
    const encodedCriteria: Partial<{ subjectId: number; predicateId: number; objectId: number }> =
      {};

    if (criteria?.subject) {
      const subjectId = this.getNodeId(criteria.subject);
      if (subjectId !== undefined) encodedCriteria.subjectId = subjectId;
      else return; // 主语不存在，返回空
    }

    if (criteria?.predicate) {
      const predicateId = this.getNodeId(criteria.predicate);
      if (predicateId !== undefined) encodedCriteria.predicateId = predicateId;
      else return; // 谓语不存在，返回空
    }

    if (criteria?.object) {
      const objectId = this.getNodeId(criteria.object);
      if (objectId !== undefined) encodedCriteria.objectId = objectId;
      else return; // 宾语不存在，返回空
    }

    // 使用底层流式查询
    for await (const batch of this.store.streamFactRecords(encodedCriteria, batchSize)) {
      if (batch.length > 0) {
        yield batch;
      }
    }
  }

  /**
   * 兼容别名：满足测试与直觉 API（与 streamFacts 等价）
   */
  findStream(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    options?: { batchSize?: number },
  ): AsyncIterable<FactRecord[]> {
    return this.streamFacts(criteria, options?.batchSize);
  }

  /**
   * 查询事实 - 支持链式操作
   */
  find(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder {
    // 快照上下文内：保持历史的"即刻物化"语义
    if (this.snapshotDepth > 0) {
      const anchor = options?.anchor ?? this.inferAnchor(criteria);
      const context = buildFindContext(this.store, criteria, anchor);
      return QueryBuilder.fromFindResult(this.store, context);
    }
    // 默认采用惰性执行
    const anchor = options?.anchor ?? this.inferAnchor(criteria);
    return new LazyQueryBuilder(this.store, criteria, anchor);
  }

  /**
   * 惰性执行版查询（灰度）：仅在执行时物化
   */
  findLazy(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder {
    return this.find(criteria, options);
  }

  /**
   * 流式查询 - 真正内存高效的大数据集查询
   */
  async findStreaming(
    criteria: FactCriteria,
    options?: { anchor?: FrontierOrientation },
  ): Promise<StreamingQueryBuilder> {
    const anchor = options?.anchor ?? this.inferAnchor(criteria);
    const pinned = this.store.getCurrentEpoch();

    // 流式查询始终使用快照模式以保证一致性
    try {
      await this.store.pushPinnedEpoch(pinned);
      const context = await buildStreamingFindContext(this.store, criteria, anchor);
      return new StreamingQueryBuilder(this.store, context, pinned);
    } finally {
      await this.store.popPinnedEpoch();
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
    const pinned = this.store.getCurrentEpoch();
    const hasPagedData = this.store.hasPagedIndexData();

    if (hasPagedData) {
      const pushPromise = this.store.pushPinnedEpoch(pinned);
      void pushPromise.catch(() => undefined);
      try {
        const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'node');
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        const popPromise = this.store.popPinnedEpoch();
        void popPromise.catch(() => undefined);
      }
    }

    const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'node');
    return QueryBuilder.fromFindResult(this.store, context);
  }

  /**
   * 基于边属性进行查询
   */
  findByEdgeProperty(
    propertyFilter: PropertyFilter,
    options?: { anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const pinned = this.store.getCurrentEpoch();
    const hasPagedData = this.store.hasPagedIndexData();

    if (hasPagedData) {
      const pushPromise = this.store.pushPinnedEpoch(pinned);
      void pushPromise.catch(() => undefined);
      try {
        const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'edge');
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        const popPromise = this.store.popPinnedEpoch();
        void popPromise.catch(() => undefined);
      }
    }

    const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'edge');
    return QueryBuilder.fromFindResult(this.store, context);
  }

  /**
   * 基于节点标签进行查询
   */
  findByLabel(
    labels: string | string[],
    options?: { mode?: 'AND' | 'OR'; anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const labelOptions = options?.mode ? { mode: options.mode } : undefined;
    const pinned = this.store.getCurrentEpoch();
    const hasPagedData = this.store.hasPagedIndexData();

    if (hasPagedData) {
      const pushPromise = this.store.pushPinnedEpoch(pinned);
      void pushPromise.catch(() => undefined);
      try {
        const context = buildFindContextFromLabel(this.store, labels, labelOptions, anchor);
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        const popPromise = this.store.popPinnedEpoch();
        void popPromise.catch(() => undefined);
      }
    }

    const context = buildFindContextFromLabel(this.store, labels, labelOptions, anchor);
    return QueryBuilder.fromFindResult(this.store, context);
  }

  /**
   * 模式匹配查询（图模式）
   */
  pattern(): PatternBuilder {
    return new PatternBuilder(this.store);
  }

  /**
   * 向后兼容别名
   */
  match(): PatternBuilder {
    return this.pattern();
  }

  /**
   * 聚合查询管道
   */
  aggregate(): AggregationPipeline {
    return new AggregationPipeline(this.store);
  }

  /**
   * Cypher 查询（实验性功能）
   * 注意：这是异步接口，使用 cypherQuery 的简化版本
   */
  async cypher(
    query: string,
    params?: Record<string, unknown>,
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    return this.cypherQuery(query, params || {}, options);
  }

  /**
   * 删除事实
   */
  deleteFact(fact: FactInput): void {
    this.store.deleteFact(fact);
  }

  // ===================
  // 节点与属性 API
  // ===================

  /**
   * 获取节点ID
   */
  getNodeId(value: string): number | undefined {
    return this.store.getNodeIdByValue(value);
  }

  /**
   * 获取节点值
   */
  getNodeValue(id: number): string | undefined {
    return this.store.getNodeValueById(id);
  }

  /**
   * 获取节点属性
   */
  getNodeProperties(nodeId: number): Record<string, unknown> | null {
    const v = this.store.getNodeProperties(nodeId);
    return v ?? null;
  }

  /**
   * 获取边属性
   */
  getEdgeProperties(key: TripleKey): Record<string, unknown> | null {
    const v = this.store.getEdgeProperties(key);
    return v ?? null;
  }

  /**
   * 设置节点属性
   */
  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    this.store.setNodeProperties(nodeId, properties);
  }

  /**
   * 设置边属性
   */
  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    this.store.setEdgeProperties(key, properties);
  }

  // ===================
  // 事务 API
  // ===================

  /**
   * 开始事务批次
   */
  beginBatch(options?: BeginBatchOptions): void {
    this.store.beginBatch(options);
  }

  /**
   * 提交事务批次
   */
  commitBatch(options?: CommitBatchOptions): void {
    this.store.commitBatch(options);
  }

  /**
   * 回滚事务批次
   */
  abortBatch(): void {
    this.store.abortBatch();
  }

  /**
   * 快照隔离：在快照上下文中执行操作
   */
  async snapshot<T>(fn: (db: SynapseDB) => Promise<T>): Promise<T> {
    this.snapshotDepth++;
    const pinned = this.store.getCurrentEpoch();
    await this.store.pushPinnedEpoch(pinned);
    try {
      return await fn(this);
    } finally {
      await this.store.popPinnedEpoch();
      this.snapshotDepth--;
    }
  }

  /**
   * 向后兼容别名
   */
  async withSnapshot<T>(fn: (db: SynapseDB) => Promise<T>): Promise<T> {
    return this.snapshot(fn);
  }

  // ===================
  // 生命周期 API
  // ===================

  /**
   * 刷新到磁盘
   */
  async flush(): Promise<void> {
    await this.store.flush();
  }

  /**
   * 关闭数据库（包括清理插件）
   */
  async close(): Promise<void> {
    await this.pluginManager.cleanup();
    await this.store.close();
  }

  // ===================
  // 插件 API
  // ===================

  /**
   * 获取插件
   */
  plugin<T extends SynapseDBPlugin>(name: string): T | undefined {
    return this.pluginManager.get<T>(name);
  }

  /**
   * 检查插件是否可用
   */
  hasPlugin(name: string): boolean {
    return this.pluginManager.has(name);
  }

  /**
   * 列出所有插件
   */
  listPlugins(): Array<{ name: string; version: string }> {
    return this.pluginManager.list();
  }

  /**
   * 获取底层存储（供插件使用）
   */
  getStore(): PersistentStore {
    return this.store;
  }

  /**
   * 暂存层指标（实验性）：仅用于观测与基准
   */
  getStagingMetrics(): { lsmMemtable: number } {
    return this.store.getStagingMetrics();
  }

  // ===================
  // 高级查询 API：路径查找
  // ===================

  /**
   * 最短路径：基于 BFS，返回边序列（不存在则返回 null）
   */
  shortestPath(
    from: string,
    to: string,
    options?: {
      predicates?: string[];
      maxHops?: number;
      direction?: 'forward' | 'reverse' | 'both';
    },
  ): FactRecord[] | null {
    const plugin = this.pluginManager.get<PathfindingPlugin>('pathfinding');
    if (!plugin) throw new Error('Pathfinding plugin not available');
    return plugin.shortestPath(from, to, options);
  }

  /**
   * 双向 BFS 最短路径（无权），对大图更高效 - 优化版本
   */
  shortestPathBidirectional(
    from: string,
    to: string,
    options?: {
      predicates?: string[];
      maxHops?: number;
    },
  ): FactRecord[] | null {
    const plugin = this.pluginManager.get<PathfindingPlugin>('pathfinding');
    if (!plugin) throw new Error('Pathfinding plugin not available');
    return plugin.shortestPathBidirectional(from, to, options);
  }

  /**
   * Dijkstra 加权最短路径（权重来自边属性，默认字段 'weight'，缺省视为1）
   */
  shortestPathWeighted(
    from: string,
    to: string,
    options?: { predicate?: string; weightProperty?: string },
  ): FactRecord[] | null {
    const plugin = this.pluginManager.get<PathfindingPlugin>('pathfinding');
    if (!plugin) throw new Error('Pathfinding plugin not available');
    return plugin.shortestPathWeighted(from, to, options);
  }

  // ===================
  // Cypher 查询扩展 API
  // ===================

  /**
   * 执行 Cypher 查询（标准异步接口）
   */
  async cypherQuery(
    statement: string,
    parameters: Record<string, unknown> = {},
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    return this.requireCypherPlugin().cypherQuery(statement, parameters, options);
  }

  /**
   * 执行只读 Cypher 查询
   */
  async cypherRead(
    statement: string,
    parameters: Record<string, unknown> = {},
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    return this.requireCypherPlugin().cypherRead(statement, parameters, options);
  }

  /**
   * 验证 Cypher 语法
   */
  validateCypher(statement: string): { valid: boolean; errors: string[] } {
    return this.requireCypherPlugin().validateCypher(statement);
  }

  /**
   * 清理 Cypher 优化器缓存
   */
  clearCypherCache(): void {
    return this.requireCypherPlugin().clearCypherOptimizationCache();
  }

  /**
   * 私有方法：获取并验证 Cypher 插件
   */
  private requireCypherPlugin(): CypherPlugin {
    if (!this.hasCypherPlugin) {
      throw new Error(
        'Cypher 插件未启用。请在 open() 时传入 experimental.cypher = true，或设置环境变量 SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES=1。',
      );
    }
    const plugin = this.pluginManager.get<CypherPlugin>('cypher');
    if (!plugin) {
      throw new Error('Cypher 插件加载失败，请检查 experimental 配置。');
    }
    return plugin;
  }

  // ===================
  // 内部辅助方法
  // ===================

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
    if (hasObject && hasPredicate) {
      return 'subject';
    }
    if (hasObject) {
      return 'object';
    }
    return 'object';
  }
}

// 向后兼容：导出类型
export type {
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
  SynapseDBPlugin,
  FactCriteria,
  FrontierOrientation,
  PropertyFilter,
  CypherResult,
  CypherExecutionOptions,
};

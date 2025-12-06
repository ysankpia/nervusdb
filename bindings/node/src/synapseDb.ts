import { PluginManager, type NervusDBPlugin } from './plugins/base.js';
import { PathfindingPlugin } from './plugins/pathfinding.js';
import { CypherPlugin } from './plugins/cypher.js';
import { AggregationPlugin } from './plugins/aggregation.js';
import { warnExperimental } from './utils/experimental.js';
import {
  PersistentStore,
  FactInput,
  FactRecord,
  TripleKey,
  TemporalEpisodeInput,
  TemporalEpisodeLinkRecord,
  TemporalEnsureEntityOptions,
  TemporalFactWriteInput,
  TemporalStoredEpisode,
  TemporalStoredEntity,
  TemporalStoredFact,
  TemporalTimelineQuery,
} from './core/storage/persistentStore.js';
import type {
  NervusDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';
import { AggregationPipeline } from './extensions/query/aggregation.js';
import type { CypherResult, CypherExecutionOptions } from './extensions/query/cypher.js';
import type { TemporalMemoryStore } from './core/storage/temporal/temporalStore.js';

export interface FactOptions {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

// 重新导出核心类型
export type { FactInput, FactRecord };

export interface TemporalMemoryAPI {
  getStore(): TemporalMemoryStore | undefined;
  addEpisode(input: TemporalEpisodeInput): Promise<TemporalStoredEpisode>;
  ensureEntity(
    kind: string,
    canonicalName: string,
    options?: TemporalEnsureEntityOptions,
  ): Promise<TemporalStoredEntity>;
  upsertFact(input: TemporalFactWriteInput): Promise<TemporalStoredFact>;
  linkEpisode(
    episodeId: number,
    options: { entityId?: number | null; factId?: number | null; role: string },
  ): Promise<TemporalEpisodeLinkRecord>;
  timeline(query: TemporalTimelineQuery): TemporalStoredFact[];
  traceBack(factId: number): TemporalStoredEpisode[];
}

export type {
  TemporalEpisodeInput,
  TemporalEpisodeLinkRecord,
  TemporalEnsureEntityOptions,
  TemporalFactWriteInput,
  TemporalStoredEpisode,
  TemporalStoredEntity,
  TemporalStoredFact,
  TemporalTimelineQuery,
};

/**
 * NervusDB - 嵌入式三元组知识库
 *
 * 统一的知识库实现，包含：
 * - 核心存储与查询功能
 * - 插件系统（默认加载 PathfindingPlugin + AggregationPlugin）
 * - 可选实验性功能（Cypher 查询）
 *
 * "好品味"原则：简单的 API，强大的功能，没有特殊情况。
 */
export class NervusDB {
  private snapshotDepth = 0;
  private pluginManager: PluginManager;
  private hasCypherPlugin: boolean;
  public readonly memory: TemporalMemoryAPI;

  private constructor(
    private readonly store: PersistentStore,
    plugins: NervusDBPlugin[],
    hasCypher: boolean,
  ) {
    this.hasCypherPlugin = hasCypher;
    this.pluginManager = new PluginManager(this, store);
    this.memory = this.createTemporalApi();

    for (const plugin of plugins) {
      this.pluginManager.register(plugin);
    }
  }

  private createTemporalApi(): TemporalMemoryAPI {
    return {
      getStore: () => this.store.getTemporalMemory(),
      addEpisode: (input) => this.store.addEpisodeToTemporalStore(input),
      ensureEntity: (kind, canonicalName, options) =>
        this.store.ensureTemporalEntity(kind, canonicalName, options ?? {}),
      upsertFact: (input) => this.store.upsertTemporalFact(input),
      linkEpisode: (episodeId, options) => this.store.linkTemporalEpisode(episodeId, options),
      timeline: (query) => this.store.queryTemporalTimeline(query),
      traceBack: (factId) => this.store.traceTemporalFact(factId),
    };
  }

  /**
   * 打开或创建 NervusDB 数据库
   */
  static async open(path: string, options?: NervusDBOpenOptions): Promise<NervusDB> {
    const experimental = options?.experimental ?? {};
    const envEnableExperimental = process.env.SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES === '1';
    const enableCypher = experimental.cypher ?? envEnableExperimental;

    const plugins: NervusDBPlugin[] = [new PathfindingPlugin(), new AggregationPlugin()];

    if (enableCypher) {
      plugins.push(new CypherPlugin());
      warnExperimental('Cypher 查询语言前端');
    }

    const store = await PersistentStore.open(path, options ?? {});
    const db = new NervusDB(store, plugins, enableCypher);
    await db.pluginManager.initialize();

    return db;
  }

  // ===================
  // 核心 API：存储与查询
  // ===================

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

  listFacts(): FactRecord[] {
    return this.store.listFacts();
  }

  async *streamFacts(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    batchSize = 1000,
  ): AsyncGenerator<FactRecord[], void, unknown> {
    const encodedCriteria: Partial<{ subjectId: number; predicateId: number; objectId: number }> =
      {};

    if (criteria?.subject) {
      const subjectId = this.getNodeId(criteria.subject);
      if (subjectId !== undefined) encodedCriteria.subjectId = subjectId;
      else return;
    }

    if (criteria?.predicate) {
      const predicateId = this.getNodeId(criteria.predicate);
      if (predicateId !== undefined) encodedCriteria.predicateId = predicateId;
      else return;
    }

    if (criteria?.object) {
      const objectId = this.getNodeId(criteria.object);
      if (objectId !== undefined) encodedCriteria.objectId = objectId;
      else return;
    }

    for await (const batch of this.store.streamFactRecords(encodedCriteria, batchSize)) {
      if (batch.length > 0) {
        yield batch;
      }
    }
  }

  aggregate(): AggregationPipeline {
    return new AggregationPipeline(this.store);
  }

  async cypher(
    query: string,
    params?: Record<string, unknown>,
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    return this.cypherQuery(query, params || {}, options);
  }

  deleteFact(fact: FactInput): void {
    this.store.deleteFact(fact);
  }

  // ===================
  // 节点与属性 API
  // ===================

  getNodeId(value: string): number | undefined {
    return this.store.getNodeIdByValue(value);
  }

  getNodeValue(id: number): string | undefined {
    return this.store.getNodeValueById(id);
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | null {
    const v = this.store.getNodeProperties(nodeId);
    return v ?? null;
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | null {
    const v = this.store.getEdgeProperties(key);
    return v ?? null;
  }

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    this.store.setNodeProperties(nodeId, properties);
  }

  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    this.store.setEdgeProperties(key, properties);
  }

  // ===================
  // 事务 API
  // ===================

  beginBatch(options?: BeginBatchOptions): void {
    this.store.beginBatch(options);
  }

  commitBatch(options?: CommitBatchOptions): void {
    this.store.commitBatch(options);
  }

  abortBatch(): void {
    this.store.abortBatch();
  }

  async snapshot<T>(fn: (db: NervusDB) => Promise<T>): Promise<T> {
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

  async withSnapshot<T>(fn: (db: NervusDB) => Promise<T>): Promise<T> {
    return this.snapshot(fn);
  }

  // ===================
  // 生命周期 API
  // ===================

  async flush(): Promise<void> {
    await this.store.flush();
  }

  async close(): Promise<void> {
    await this.pluginManager.cleanup();
    await this.store.close();
  }

  // ===================
  // 插件 API
  // ===================

  plugin<T extends NervusDBPlugin>(name: string): T | undefined {
    return this.pluginManager.get<T>(name);
  }

  hasPlugin(name: string): boolean {
    return this.pluginManager.has(name);
  }

  listPlugins(): Array<{ name: string; version: string }> {
    return this.pluginManager.list();
  }

  getStore(): PersistentStore {
    return this.store;
  }

  getStagingMetrics(): { lsmMemtable: number } {
    return this.store.getStagingMetrics();
  }

  // ===================
  // 路径查找 API
  // ===================

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
  // 图算法 API (Rust Native)
  // ===================

  /**
   * PageRank 算法
   * 计算图中所有节点的 PageRank 分数
   *
   * @param options.predicate - 只考虑特定谓词的边（可选）
   * @param options.damping - 阻尼系数，默认 0.85
   * @param options.maxIterations - 最大迭代次数，默认 100
   * @param options.tolerance - 收敛容差，默认 1e-6
   * @returns PageRank 结果，包含每个节点的分数、迭代次数和是否收敛
   */
  pagerank(options?: {
    predicate?: string;
    damping?: number;
    maxIterations?: number;
    tolerance?: number;
  }): { scores: Array<{ nodeId: number; nodeValue: string; score: number }>; iterations: number; converged: boolean } | null {
    const nativeHandle = this.store.getNativeHandle();
    const pagerankFn = nativeHandle.pagerank;
    if (!pagerankFn) {
      // Native PageRank 不可用
      return null;
    }

    const predicateId = options?.predicate
      ? this.store.getNodeIdByValue(options.predicate)
      : undefined;

    const result = pagerankFn.call(
      nativeHandle,
      predicateId !== undefined ? BigInt(predicateId) : null,
      options?.damping ?? null,
      options?.maxIterations ?? null,
      options?.tolerance ?? null,
    );

    // 转换结果，将节点 ID 解析为值
    const scores = result.scores.map((entry: { nodeId: bigint; score: number }) => {
      const nodeId = Number(entry.nodeId);
      return {
        nodeId,
        nodeValue: this.store.getNodeValueById(nodeId) ?? `<unknown:${nodeId}>`,
        score: entry.score,
      };
    });

    return {
      scores,
      iterations: result.iterations,
      converged: result.converged,
    };
  }

  // ===================
  // Cypher 查询 API
  // ===================

  async cypherQuery(
    statement: string,
    parameters: Record<string, unknown> = {},
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    try {
      const rows = this.store.query(statement as any) as any[];
      return { records: rows, summary: { native: true } as any };
    } catch {
      return this.requireCypherPlugin().cypherQuery(statement, parameters, options);
    }
  }

  async cypherRead(
    statement: string,
    parameters: Record<string, unknown> = {},
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    return this.requireCypherPlugin().cypherRead(statement, parameters, options);
  }

  validateCypher(statement: string): { valid: boolean; errors: string[] } {
    return this.requireCypherPlugin().validateCypher(statement);
  }

  clearCypherCache(): void {
    return this.requireCypherPlugin().clearCypherOptimizationCache();
  }

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
}

export type {
  NervusDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
  NervusDBPlugin,
  CypherResult,
  CypherExecutionOptions,
};

import { warnExperimental } from './utils/experimental.js';
import {
  PersistentStore,
  FactInput,
  FactRecord,
  NativeDatabaseHandle,
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
import type { TemporalMemoryStore } from './core/storage/temporal/temporalStore.js';

export interface FactOptions {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

export type CypherRecord = Record<string, unknown>;

export interface CypherResult {
  records: CypherRecord[];
  summary: {
    statement: string;
    parameters: Record<string, unknown>;
    native: true;
  };
}

export interface CypherExecutionOptions {
  readonly?: boolean;
}

export interface NativePathResult {
  path: bigint[];
  cost: number;
  hops: number;
}

export interface NativePageRankEntry {
  nodeId: bigint;
  score: number;
}

export interface NativePageRankResult {
  scores: NativePageRankEntry[];
  iterations: number;
  converged: boolean;
}

export interface GraphAlgorithmsAPI {
  bfsShortestPath(
    startId: number | bigint,
    endId: number | bigint,
    predicateId?: number | bigint | null,
    options?: { maxHops?: number; bidirectional?: boolean },
  ): NativePathResult | null;

  dijkstraShortestPath(
    startId: number | bigint,
    endId: number | bigint,
    predicateId?: number | bigint | null,
    options?: { maxHops?: number },
  ): NativePathResult | null;

  pagerank(options?: {
    predicateId?: number | bigint | null;
    damping?: number;
    maxIterations?: number;
    tolerance?: number;
  }): NativePageRankResult | null;
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
 * 目标：薄绑定（thin binding）。
 * - TypeScript 只做参数/类型转换
 * - 所有查询/算法/执行器都在 Rust Core
 */
export class NervusDB {
  public readonly algorithms: GraphAlgorithmsAPI;
  public readonly memory: TemporalMemoryAPI;
  private readonly cypherEnabled: boolean;

  private constructor(private readonly store: PersistentStore, cypherEnabled: boolean) {
    this.cypherEnabled = cypherEnabled;
    this.algorithms = this.createAlgorithmsApi();
    this.memory = this.createTemporalApi();
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

    if (enableCypher) {
      warnExperimental('Cypher 查询语言前端');
    }

    const store = await PersistentStore.open(path, options ?? {});
    return new NervusDB(store, enableCypher);
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

  // ===================
  // 生命周期 API
  // ===================

  async flush(): Promise<void> {
    await this.store.flush();
  }

  async close(): Promise<void> {
    await this.store.close();
  }

  getStore(): PersistentStore {
    return this.store;
  }

  getStagingMetrics(): { lsmMemtable: number } {
    return this.store.getStagingMetrics();
  }

  // ===================
  // Cypher 查询 API
  // ===================

  async cypherQuery(
    statement: string,
    parameters: Record<string, unknown> = {},
    _options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    this.ensureCypherEnabled();
    const records = this.store.executeQuery(statement, parameters) as CypherRecord[];
    return {
      records,
      summary: {
        statement,
        parameters,
        native: true,
      },
    };
  }

  async cypherRead(
    statement: string,
    parameters: Record<string, unknown> = {},
    options?: CypherExecutionOptions,
  ): Promise<CypherResult> {
    return this.cypherQuery(statement, parameters, options);
  }

  private ensureCypherEnabled(): void {
    if (this.cypherEnabled) return;
    throw new Error(
      'Cypher 处于实验阶段且默认关闭。请在 open() 时传入 experimental.cypher = true，或设置 SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES=1。',
    );
  }

  private createAlgorithmsApi(): GraphAlgorithmsAPI {
    const handle = this.store.getNativeHandle();
    const must = <T>(value: T | undefined, name: string): T => {
      if (value) return value;
      throw new Error(`Native method not available: ${name} (upgrade native addon)`);
    };
    const toBigInt = (v: number | bigint) => (typeof v === 'bigint' ? v : BigInt(v));

    return {
      bfsShortestPath: (startId, endId, predicateId, options) => {
        const fn = must(handle.bfsShortestPath, 'bfsShortestPath');
        return (
          fn.call(
            handle as NativeDatabaseHandle,
            toBigInt(startId),
            toBigInt(endId),
            predicateId == null ? null : toBigInt(predicateId),
            options?.maxHops ?? null,
            options?.bidirectional ?? null,
          ) ?? null
        );
      },
      dijkstraShortestPath: (startId, endId, predicateId, options) => {
        const fn = must(handle.dijkstraShortestPath, 'dijkstraShortestPath');
        return (
          fn.call(
            handle as NativeDatabaseHandle,
            toBigInt(startId),
            toBigInt(endId),
            predicateId == null ? null : toBigInt(predicateId),
            options?.maxHops ?? null,
          ) ?? null
        );
      },
      pagerank: (options) => {
        const fn = must(handle.pagerank, 'pagerank');
        return fn.call(
          handle as NativeDatabaseHandle,
          options?.predicateId == null ? null : toBigInt(options.predicateId),
          options?.damping ?? null,
          options?.maxIterations ?? null,
          options?.tolerance ?? null,
        );
      },
    };
  }
}

export type {
  NervusDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
};

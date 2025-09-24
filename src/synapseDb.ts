import { PersistentStore, FactInput, FactRecord } from './storage/persistentStore.js';
import { TripleKey } from './storage/propertyStore.js';
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
} from './query/queryBuilder.js';
import {
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';
import { AggregationPipeline } from './query/aggregation.js';
import { VariablePathBuilder } from './query/path/variable.js';
import { PatternBuilder } from './query/pattern/match.js';
import { MinHeap } from './utils/minHeap.js';
// Cypher 支持（异步 API）
import {
  createCypherSupport,
  type CypherSupport,
  type CypherResult,
  type CypherExecutionOptions,
} from './query/cypher.js';

export interface FactOptions {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

/**
 * SynapseDB - 嵌入式三元组知识库
 *
 * 基于 TypeScript 实现的类 SQLite 单文件数据库，专门用于存储和查询 SPO 三元组数据。
 * 支持分页索引、WAL 事务、快照一致性、自动压缩和垃圾回收。
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
  private constructor(private readonly store: PersistentStore) {}
  // 延迟创建的 Cypher 支持实例
  private _cypherSupport?: CypherSupport;
  private snapshotDepth = 0;

  /**
   * 获取（或延迟创建）Cypher 支持实例
   */
  private getCypherSupport(): CypherSupport {
    if (!this._cypherSupport) {
      this._cypherSupport = createCypherSupport(this.store);
    }
    return this._cypherSupport;
  }

  /**
   * 打开或创建 SynapseDB 数据库
   *
   * @param path 数据库文件路径，如果不存在将自动创建
   * @param options 数据库配置选项
   * @returns Promise<SynapseDB> 数据库实例
   *
   * @example
   * ```typescript
   * // 基本用法
   * const db = await SynapseDB.open('./my-database.synapsedb');
   *
   * // 带配置的用法
   * const db = await SynapseDB.open('./my-database.synapsedb', {
   *   pageSize: 1500,
   *   enableLock: true,
   *   registerReader: true,
   *   compression: { codec: 'brotli', level: 4 }
   * });
   * ```
   *
   * @throws {Error} 当文件无法访问或锁定冲突时
   */
  static async open(path: string, options?: SynapseDBOpenOptions): Promise<SynapseDB> {
    const store = await PersistentStore.open(path, options ?? {});
    return new SynapseDB(store);
  }

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

  // 流式查询：逐批返回事实记录，避免大结果集内存压力
  async *streamFacts(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    batchSize = 1000,
  ): AsyncGenerator<FactRecord[], void, unknown> {
    // 将字符串条件转换为ID条件
    const encodedCriteria: Partial<{ subjectId: number; predicateId: number; objectId: number }> =
      {};

    if (criteria?.subject) {
      const subjectId = this.store.getNodeIdByValue(criteria.subject);
      if (subjectId !== undefined) encodedCriteria.subjectId = subjectId;
      else return; // 主语不存在，返回空
    }

    if (criteria?.predicate) {
      const predicateId = this.store.getNodeIdByValue(criteria.predicate);
      if (predicateId !== undefined) encodedCriteria.predicateId = predicateId;
      else return; // 谓语不存在，返回空
    }

    if (criteria?.object) {
      const objectId = this.store.getNodeIdByValue(criteria.object);
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

  // 兼容别名：满足测试与直觉 API（与 streamFacts 等价）
  findStream(
    criteria?: Partial<{ subject: string; predicate: string; object: string }>,
    options?: { batchSize?: number },
  ): AsyncIterable<FactRecord[]> {
    return this.streamFacts(criteria, options?.batchSize);
  }

  getNodeId(value: string): number | undefined {
    return this.store.getNodeIdByValue(value);
  }

  getNodeValue(id: number): string | undefined {
    return this.store.getNodeValueById(id);
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | null {
    const v = this.store.getNodeProperties(nodeId);
    // 对外 API 约定：未设置返回 null，便于测试与调用方判空
    return v ?? null;
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | null {
    const v = this.store.getEdgeProperties(key);
    return v ?? null;
  }

  async flush(): Promise<void> {
    await this.store.flush();
  }

  /**
   * 流式查询 - 真正内存高效的大数据集查询
   * @param criteria 查询条件
   * @param options 查询选项
   * @returns StreamingQueryBuilder 支持异步迭代，内存占用恒定
   * @example
   * ```typescript
   * // 流式处理大数据集，内存占用恒定
   * for await (const fact of db.findStreaming({ predicate: 'HAS_METHOD' })) {
   *   console.log(fact);
   * }
   * ```
   */
  async findStreaming(
    criteria: FactCriteria,
    options?: { anchor?: FrontierOrientation },
  ): Promise<StreamingQueryBuilder> {
    const anchor = options?.anchor ?? inferAnchor(criteria);
    const pinned =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;

    // 流式查询始终使用快照模式以保证一致性
    try {
      (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(pinned);
      const context = await buildStreamingFindContext(this.store, criteria, anchor);
      return new StreamingQueryBuilder(this.store, context, pinned);
    } finally {
      (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
    }
  }

  find(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder {
    const anchor = options?.anchor ?? inferAnchor(criteria);
    const pinned =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;

    // 检查是否有分页索引数据，如果没有则不使用快照模式
    const pagedReaders = (
      this.store as unknown as {
        pagedReaders: Map<string, { getPrimaryValues?: () => number[] }>;
      }
    ).pagedReaders;
    let hasPagedData = false;
    if (pagedReaders?.size > 0) {
      // 检查索引是否真的包含数据
      const spoReader = pagedReaders.get('SPO');
      if (spoReader) {
        const primaryValues = spoReader.getPrimaryValues?.() ?? [];
        hasPagedData = primaryValues.length > 0;
      }
    }

    const isEmptyCriteria =
      criteria.subject === undefined &&
      criteria.predicate === undefined &&
      criteria.object === undefined;

    if (hasPagedData) {
      // 有分页索引数据时，使用快照模式保证一致性
      try {
        (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(
          pinned,
        );
        // 快照期间的空条件（全量扫描）在分页索引场景下改为返回空上下文，
        // 由上层选择流式API或限制片段，避免一次性加载占用大量内存。
        // 非快照场景保持完整行为（用于 WAL/事务相关测试）。
        const context =
          isEmptyCriteria && this.snapshotDepth > 0
            ? { facts: [], frontier: new Set<number>(), orientation: anchor }
            : buildFindContext(this.store, criteria, anchor);
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
      }
    } else {
      // 没有分页索引数据时，直接使用常规查询（不设置快照）
      const context = buildFindContext(this.store, criteria, anchor);
      return QueryBuilder.fromFindResult(this.store, context);
    }
  }

  /**
   * 基于节点属性进行查询
   * @param propertyFilter 属性过滤条件
   * @param options 查询选项
   * @example
   * ```typescript
   * // 查找所有年龄为25的用户
   * const users = db.findByNodeProperty(
   *   { propertyName: 'age', value: 25 },
   *   { anchor: 'subject' }
   * ).all();
   *
   * // 查找年龄在25-35之间的用户
   * const adults = db.findByNodeProperty({
   *   propertyName: 'age',
   *   range: { min: 25, max: 35, includeMin: true, includeMax: true }
   * }).all();
   * ```
   */
  findByNodeProperty(
    propertyFilter: PropertyFilter,
    options?: { anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const pinned =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;

    // 检查是否有分页索引数据
    const pagedReaders = (
      this.store as unknown as {
        pagedReaders: Map<string, { getPrimaryValues?: () => number[] }>;
      }
    ).pagedReaders;
    let hasPagedData = false;
    if (pagedReaders?.size > 0) {
      const spoReader = pagedReaders.get('SPO');
      if (spoReader) {
        const primaryValues = spoReader.getPrimaryValues?.() ?? [];
        hasPagedData = primaryValues.length > 0;
      }
    }

    if (hasPagedData) {
      try {
        (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(
          pinned,
        );
        const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'node');
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
      }
    } else {
      const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'node');
      return QueryBuilder.fromFindResult(this.store, context);
    }
  }

  /**
   * 基于边属性进行查询
   * @param propertyFilter 属性过滤条件
   * @param options 查询选项
   * @example
   * ```typescript
   * // 查找所有权重为0.8的关系
   * const strongRelations = db.findByEdgeProperty(
   *   { propertyName: 'weight', value: 0.8 }
   * ).all();
   * ```
   */
  findByEdgeProperty(
    propertyFilter: PropertyFilter,
    options?: { anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const pinned =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;

    // 检查是否有分页索引数据
    const pagedReaders = (
      this.store as unknown as {
        pagedReaders: Map<string, { getPrimaryValues?: () => number[] }>;
      }
    ).pagedReaders;
    let hasPagedData = false;
    if (pagedReaders?.size > 0) {
      const spoReader = pagedReaders.get('SPO');
      if (spoReader) {
        const primaryValues = spoReader.getPrimaryValues?.() ?? [];
        hasPagedData = primaryValues.length > 0;
      }
    }

    if (hasPagedData) {
      try {
        (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(
          pinned,
        );
        const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'edge');
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
      }
    } else {
      const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'edge');
      return QueryBuilder.fromFindResult(this.store, context);
    }
  }

  /**
   * 基于节点标签进行查询
   * @param labels 单个或多个标签
   * @param options 查询选项：{ mode?: 'AND' | 'OR', anchor?: 'subject'|'object'|'both' }
   */
  findByLabel(
    labels: string | string[],
    options?: { mode?: 'AND' | 'OR'; anchor?: FrontierOrientation },
  ): QueryBuilder {
    const anchor = options?.anchor ?? 'subject';
    const pinned =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;

    // 同 find()/属性查询：如果已有分页索引数据，采用快照模式
    const pagedReaders = (
      this.store as unknown as {
        pagedReaders: Map<string, { getPrimaryValues?: () => number[] }>;
      }
    ).pagedReaders;
    let hasPagedData = false;
    if (pagedReaders?.size > 0) {
      const spoReader = pagedReaders.get('SPO');
      if (spoReader) {
        const primaryValues = spoReader.getPrimaryValues?.() ?? [];
        hasPagedData = primaryValues.length > 0;
      }
    }

    if (hasPagedData) {
      try {
        (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(
          pinned,
        );
        const context = buildFindContextFromLabel(
          this.store,
          labels,
          { mode: options?.mode },
          anchor,
        );
        return QueryBuilder.fromFindResult(this.store, context, pinned);
      } finally {
        (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
      }
    } else {
      const context = buildFindContextFromLabel(
        this.store,
        labels,
        { mode: options?.mode },
        anchor,
      );
      return QueryBuilder.fromFindResult(this.store, context);
    }
  }

  deleteFact(fact: FactInput): void {
    this.store.deleteFact(fact);
  }

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    this.store.setNodeProperties(nodeId, properties);
  }

  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    this.store.setEdgeProperties(key, properties);
  }

  // 事务批次控制（可选）：允许将多次写入合并为一次提交
  beginBatch(options?: BeginBatchOptions): void {
    this.store.beginBatch(options);
  }

  commitBatch(options?: CommitBatchOptions): void {
    this.store.commitBatch(options);
  }

  abortBatch(): void {
    this.store.abortBatch();
  }

  async close(): Promise<void> {
    await this.store.close();
  }

  // 读快照：在给定回调期间固定当前 epoch，避免 mid-chain 刷新 readers 造成视图漂移
  async withSnapshot<T>(fn: (db: SynapseDB) => Promise<T> | T): Promise<T> {
    const epoch =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;
    try {
      this.snapshotDepth++;
      // 等待读者注册完成，确保快照安全
      await (
        this.store as unknown as { pushPinnedEpoch: (e: number) => Promise<void> }
      ).pushPinnedEpoch?.(epoch);
      return await fn(this);
    } finally {
      await (this.store as unknown as { popPinnedEpoch: () => Promise<void> }).popPinnedEpoch?.();
      this.snapshotDepth = Math.max(0, this.snapshotDepth - 1);
    }
  }

  // 暂存层指标（实验性）：仅用于观测与基准
  getStagingMetrics(): { lsmMemtable: number } {
    return (
      (
        this.store as unknown as { getStagingMetrics: () => { lsmMemtable: number } }
      ).getStagingMetrics?.() ?? { lsmMemtable: 0 }
    );
  }

  // 聚合入口
  aggregate(): AggregationPipeline {
    return new AggregationPipeline(this.store);
  }

  // 模式匹配入口（最小实现）
  match(): PatternBuilder {
    return new PatternBuilder(this.store);
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
  ): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;
    const dir = options?.direction ?? 'forward';
    const maxHops = Math.max(1, options?.maxHops ?? 8);
    const predIds: number[] | null = options?.predicates
      ? options.predicates
          .map((p) => this.store.getNodeIdByValue(p))
          .filter((x): x is number => typeof x === 'number')
      : null;

    const qNeighbors = (nid: number): FactRecord[] => {
      const outs: FactRecord[] = [];
      const pushMatches = (
        criteria: Partial<{ subjectId: number; predicateId: number; objectId: number }>,
      ) => {
        const enc = this.store.query(criteria);
        outs.push(...this.store.resolveRecords(enc));
      };
      if (dir === 'forward' || dir === 'both') {
        if (predIds && predIds.length > 0) {
          for (const pid of predIds) pushMatches({ subjectId: nid, predicateId: pid });
        } else {
          pushMatches({ subjectId: nid });
        }
      }
      if (dir === 'reverse' || dir === 'both') {
        if (predIds && predIds.length > 0) {
          for (const pid of predIds) pushMatches({ predicateId: pid, objectId: nid });
        } else {
          pushMatches({ objectId: nid });
        }
      }
      return outs;
    };

    const queue: Array<{ node: number; path: FactRecord[] }> = [{ node: startId, path: [] }];
    const visited = new Set<number>([startId]);
    let depth = 0;
    while (queue.length > 0 && depth <= maxHops) {
      const levelSize = queue.length;
      for (let i = 0; i < levelSize; i++) {
        const cur = queue.shift()!;
        if (cur.node === endId) return cur.path;
        const neigh = qNeighbors(cur.node);
        for (const e of neigh) {
          const nextNode = e.subjectId === cur.node ? e.objectId : e.subjectId;
          if (visited.has(nextNode)) continue;
          visited.add(nextNode);
          queue.push({ node: nextNode, path: [...cur.path, e] });
        }
      }
      depth += 1;
    }
    return null;
  }

  // 双向 BFS 最短路径（无权），对大图更高效 - 优化版本
  shortestPathBidirectional(
    from: string,
    to: string,
    options?: {
      predicates?: string[];
      maxHops?: number;
    },
  ): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;
    if (startId === endId) return [];

    const maxHops = Math.max(1, options?.maxHops ?? 8);
    const predIds: number[] | null = options?.predicates
      ? options.predicates
          .map((p) => this.store.getNodeIdByValue(p))
          .filter((x): x is number => typeof x === 'number')
      : null;

    // 缓存查询结果，避免重复查询相同节点
    const forwardCache = new Map<number, FactRecord[]>();
    const backwardCache = new Map<number, FactRecord[]>();

    const neighborsForward = (nid: number): FactRecord[] => {
      if (forwardCache.has(nid)) {
        return forwardCache.get(nid)!;
      }

      const out: FactRecord[] = [];
      const pushMatches = (
        criteria: Partial<{ subjectId: number; predicateId: number; objectId: number }>,
      ) => {
        const enc = this.store.query(criteria);
        out.push(...this.store.resolveRecords(enc));
      };

      if (predIds && predIds.length > 0) {
        for (const pid of predIds) pushMatches({ subjectId: nid, predicateId: pid });
      } else {
        pushMatches({ subjectId: nid });
      }

      forwardCache.set(nid, out);
      return out;
    };

    const neighborsBackward = (nid: number): FactRecord[] => {
      if (backwardCache.has(nid)) {
        return backwardCache.get(nid)!;
      }

      const out: FactRecord[] = [];
      const pushMatches = (
        criteria: Partial<{ subjectId: number; predicateId: number; objectId: number }>,
      ) => {
        const enc = this.store.query(criteria);
        out.push(...this.store.resolveRecords(enc));
      };

      if (predIds && predIds.length > 0) {
        for (const pid of predIds) pushMatches({ predicateId: pid, objectId: nid });
      } else {
        pushMatches({ objectId: nid });
      }

      backwardCache.set(nid, out);
      return out;
    };

    // 使用 Map 提高查找效率，避免数组的线性搜索
    const prevFrom = new Map<number, FactRecord>();
    const nextTo = new Map<number, FactRecord>();

    const visitedFrom = new Set<number>([startId]);
    const visitedTo = new Set<number>([endId]);
    let frontierFrom = new Set<number>([startId]);
    let frontierTo = new Set<number>([endId]);

    let hops = 0;
    let meet: number | null = null;

    while (frontierFrom.size > 0 && frontierTo.size > 0 && hops < maxHops / 2 + 1) {
      hops += 1;

      // 选择较小的一侧扩展，提高效率
      if (frontierFrom.size <= frontierTo.size) {
        const nextFrontier = new Set<number>();
        for (const u of frontierFrom) {
          const neighbors = neighborsForward(u);
          for (const e of neighbors) {
            const v = e.objectId;
            if (visitedFrom.has(v)) continue;

            visitedFrom.add(v);
            prevFrom.set(v, e);

            // 检查是否与另一侧相遇
            if (visitedTo.has(v)) {
              meet = v;
              break;
            }

            nextFrontier.add(v);
          }
          if (meet !== null) break;
        }
        if (meet !== null) break;
        frontierFrom = nextFrontier;
      } else {
        const nextFrontier = new Set<number>();
        for (const u of frontierTo) {
          const neighbors = neighborsBackward(u);
          for (const e of neighbors) {
            const v = e.subjectId; // 反向扩展得到上一节点
            if (visitedTo.has(v)) continue;

            visitedTo.add(v);
            nextTo.set(v, e);

            // 检查是否与另一侧相遇
            if (visitedFrom.has(v)) {
              meet = v;
              break;
            }

            nextFrontier.add(v);
          }
          if (meet !== null) break;
        }
        if (meet !== null) break;
        frontierTo = nextFrontier;
      }
    }

    if (meet === null) return null;

    // 优化路径重建：使用单次遍历构建完整路径
    const path: FactRecord[] = [];

    // 回溯 start -> meet
    const leftPath: FactRecord[] = [];
    let cur = meet;
    while (cur !== startId && prevFrom.has(cur)) {
      const e = prevFrom.get(cur)!;
      leftPath.push(e);
      cur = e.subjectId;
    }

    // 正向遍历 start -> meet
    for (let i = leftPath.length - 1; i >= 0; i--) {
      path.push(leftPath[i]);
    }

    // 正向拼接 meet -> end
    cur = meet;
    while (cur !== endId && nextTo.has(cur)) {
      const e = nextTo.get(cur)!;
      path.push(e);
      cur = e.objectId;
    }

    return path;
  }

  // Dijkstra 加权最短路径（权重来自边属性，默认字段 'weight'，缺省视为1）
  shortestPathWeighted(
    from: string,
    to: string,
    options?: { predicate?: string; weightProperty?: string },
  ): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;
    const predicateId = options?.predicate
      ? this.store.getNodeIdByValue(options.predicate)
      : undefined;
    const weightKey = options?.weightProperty ?? 'weight';

    const dist = new Map<number, number>();
    const prev = new Map<number, FactRecord | null>();
    const visited = new Set<number>();
    dist.set(startId, 0);

    // 使用最小堆优化优先队列性能
    const queue = new MinHeap<{ node: number; d: number }>((a, b) => a.d - b.d);
    queue.push({ node: startId, d: 0 });

    while (!queue.isEmpty()) {
      const { node } = queue.pop()!;
      if (visited.has(node)) continue;
      visited.add(node);
      if (node === endId) break;

      const criteria: { subjectId: number; predicateId?: number } =
        predicateId !== undefined ? { subjectId: node, predicateId } : { subjectId: node };
      const enc = this.store.query(criteria);
      const edges = this.store.resolveRecords(enc);
      for (const e of edges) {
        const rawWeight = e.edgeProperties ? e.edgeProperties[weightKey] : undefined;
        const w = Number(rawWeight ?? 1);
        const alt = (dist.get(node) ?? Infinity) + (Number.isFinite(w) ? w : 1);
        const v = e.objectId;
        if (alt < (dist.get(v) ?? Infinity)) {
          dist.set(v, alt);
          prev.set(v, e);
          queue.push({ node: v, d: alt });
        }
      }
    }

    if (!dist.has(endId)) return null;
    const path: FactRecord[] = [];
    let cur = endId;
    while (cur !== startId) {
      const edge = prev.get(cur);
      if (!edge) break;
      path.push(edge);
      cur = edge.subjectId;
    }
    path.reverse();
    return path;
  }

  // Cypher 极简子集：仅支持 MATCH (a)-[:REL]->(b) RETURN a,b
  cypher(query: string): Array<Record<string, unknown>> {
    const m =
      /MATCH\s*\((\w+)\)\s*-\s*\[:(\w+)(?:\*(\d+)?\.\.(\d+)?)?\]\s*->\s*\((\w+)\)\s*RETURN\s+(.+)/i.exec(
        query,
      );
    if (!m) throw new Error('仅支持最小子集：MATCH (a)-[:REL]->(b) RETURN ...');
    const aliasA = m[1];
    const rel = m[2];
    const minStr = m[3];
    const maxStr = m[4];
    const aliasB = m[5];
    const returnList = m[6].split(',').map((s) => s.trim());

    const hasVar = Boolean(minStr || maxStr);
    if (!hasVar) {
      const rows = this.find({ predicate: rel }).all();
      return rows.map((r) => {
        const env: Record<string, unknown> = {};
        const mapping: Record<string, string> = {
          [aliasA]: r.subject,
          [aliasB]: r.object,
        };
        for (const item of returnList) env[item] = mapping[item] ?? null;
        return env;
      });
    }

    const min = minStr ? Number(minStr) : 1;
    const max = maxStr ? Number(maxStr) : min;
    const pid = this.store.getNodeIdByValue(rel);
    if (pid === undefined) return [];

    const startIds = new Set<number>();
    const triples = this.find({ predicate: rel }).all();
    triples.forEach((t) => startIds.add(t.subjectId));

    const builder = new VariablePathBuilder(this.store, startIds, pid, {
      min,
      max,
      uniqueness: 'NODE',
      direction: 'forward',
    });
    const paths = builder.all();
    const out: Array<Record<string, unknown>> = [];
    for (const p of paths) {
      const env: Record<string, unknown> = {};
      const mapping: Record<string, string | null> = {
        [aliasA]: this.store.getNodeValueById(p.startId) ?? null,
        [aliasB]: this.store.getNodeValueById(p.endId) ?? null,
      };
      for (const item of returnList) env[item] = mapping[item] ?? null;
      out.push(env);
    }
    return out;
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
    const cypher = this.getCypherSupport();
    return cypher.cypher(statement, parameters, options);
  }

  /**
   * 执行只读 Cypher 查询
   */
  async cypherRead(
    statement: string,
    parameters: Record<string, unknown> = {},
    options: CypherExecutionOptions = {},
  ): Promise<CypherResult> {
    const cypher = this.getCypherSupport();
    return cypher.cypherRead(statement, parameters, options);
  }

  /**
   * 验证 Cypher 语法
   */
  validateCypher(statement: string): { valid: boolean; errors: string[] } {
    const cypher = this.getCypherSupport();
    return cypher.validateCypher(statement);
  }

  /** 清理 Cypher 优化器缓存 */
  clearCypherOptimizationCache(): void {
    const cypher = this.getCypherSupport();
    cypher.clearOptimizationCache();
  }

  /** 获取 Cypher 优化器统计信息 */
  getCypherOptimizerStats(): unknown {
    const cypher = this.getCypherSupport();
    return cypher.getOptimizerStats();
  }

  /** 预热 Cypher 优化器 */
  async warmUpCypherOptimizer(): Promise<void> {
    const cypher = this.getCypherSupport();
    await cypher.warmUpOptimizer();
  }
}

export type {
  FactInput,
  FactRecord,
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
  PropertyFilter,
  FrontierOrientation,
};

function inferAnchor(criteria: FactCriteria): FrontierOrientation {
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

import { PersistentStore, FactInput, FactRecord } from './storage/persistentStore.js';
import { TripleKey } from './storage/propertyStore.js';
import {
  FactCriteria,
  FrontierOrientation,
  QueryBuilder,
  buildFindContext,
} from './query/queryBuilder.js';
import {
  SynapseDBOpenOptions,
  CommitBatchOptions,
  BeginBatchOptions,
} from './types/openOptions.js';

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

  find(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder {
    const anchor = options?.anchor ?? inferAnchor(criteria);
    const pinned =
      (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.() ?? 0;
    // 对初始 find 也进行临时 pinned 保障
    try {
      (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(pinned);
      const context = buildFindContext(this.store, criteria, anchor);
      return QueryBuilder.fromFindResult(this.store, context, pinned);
    } finally {
      (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
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
      // 等待读者注册完成，确保快照安全
      await (
        this.store as unknown as { pushPinnedEpoch: (e: number) => Promise<void> }
      ).pushPinnedEpoch?.(epoch);
      return await fn(this);
    } finally {
      await (this.store as unknown as { popPinnedEpoch: () => Promise<void> }).popPinnedEpoch?.();
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
}

export type { FactInput, FactRecord, SynapseDBOpenOptions, CommitBatchOptions, BeginBatchOptions };

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

import { EncodedTriple } from '../tripleStore.js';
import { FactRecord } from '../persistentStore.js';
import { PagedIndexCoordinator } from './pagedIndexCoordinator.js';
import { ConcurrencyControl } from './concurrencyControl.js';
import { StringDictionary } from '../dictionary.js';
import { PropertyStore } from '../propertyStore.js';
import { encodeTripleKey, matchCriteria, primaryKey } from '../helpers/tripleOrdering.js';
import { getBestIndexKey } from '../tripleIndexes.js';
import type { IndexOrder } from '../tripleIndexes.js';

export interface QueryOptions {
  includeProperties?: boolean;
  batchSize?: number;
  useSnapshot?: boolean;
}

export interface QueryContext {
  dictionary: StringDictionary;
  properties: PropertyStore;
  pagedIndex: PagedIndexCoordinator;
  concurrency: ConcurrencyControl;
  getMemoryTriples: () => EncodedTriple[];
  tombstones: Set<string>;
  bumpHot?: (order: IndexOrder, primary: number) => void;
  // 架构重构（Issue #7）：添加propertyIndexManager用于读取属性
  propertyIndexManager?: {
    getNodePropertiesSync: (nodeId: number) => Record<string, unknown> | undefined;
    getEdgePropertiesSync: (key: {
      subjectId: number;
      predicateId: number;
      objectId: number;
    }) => Record<string, unknown> | undefined;
  };
}

/**
 * 统一查询引擎：消除 PersistentStore 中的查询逻辑复杂性
 *
 * "好品味"原则：一个查询接口处理所有场景，没有特殊情况
 */
export class QueryEngine {
  constructor(private readonly context: QueryContext) {}

  /**
   * 统一查询入口：自动选择最优策略
   */
  query(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    // 快照模式：使用纯磁盘查询
    if (this.context.concurrency.hasPinnedEpoch()) {
      return this.queryFromDisk(criteria);
    }

    // 刷新检查（非快照模式）
    if (this.context.concurrency.shouldRefreshReaders()) {
      void this.refreshReadersIfNeeded();
      this.context.concurrency.updateManifestCheck();
    }

    return this.queryFromMemoryAndDisk(criteria);
  }

  /**
   * 流式查询：统一接口，自动选择同步或异步
   */
  async *streamQuery(
    criteria: Partial<EncodedTriple>,
    options: QueryOptions = {},
  ): AsyncGenerator<EncodedTriple[], void, unknown> {
    const batchSize = options.batchSize ?? 1000;

    // 快照模式：纯磁盘流式
    if (this.context.concurrency.hasPinnedEpoch()) {
      yield* this.streamFromDisk(criteria, batchSize);
      return;
    }

    // 正常模式：内存+磁盘流式
    yield* this.streamFromMemoryAndDisk(criteria, batchSize);
  }

  /**
   * 解析记录：统一的三元组到记录转换
   */
  resolveRecords(triples: EncodedTriple[], options: QueryOptions = {}): FactRecord[] {
    const includeProps = options.includeProperties !== false;
    const seen = new Set<string>();
    const results: FactRecord[] = [];

    for (const t of triples) {
      const key = encodeTripleKey(t);
      if (this.context.tombstones.has(key) || seen.has(key)) continue;
      seen.add(key);
      results.push(this.toFactRecord(t, includeProps));
    }

    return results;
  }

  /**
   * 流式记录查询：解析后的记录批次
   */
  async *streamFactRecords(
    criteria: Partial<EncodedTriple> = {},
    options: QueryOptions = {},
  ): AsyncGenerator<FactRecord[], void, unknown> {
    const batchSize = options.batchSize ?? 1000;
    for await (const tripleBatch of this.streamQuery(criteria, { ...options, batchSize })) {
      yield this.resolveRecords(tripleBatch, options);
    }
  }

  /**
   * 内存+磁盘混合查询（正常模式）
   */
  private queryFromMemoryAndDisk(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    const isFullScan = this.isFullScanQuery(criteria);

    if (isFullScan) {
      return this.executeFullScan();
    }

    return this.executeIndexedQuery(criteria);
  }

  /**
   * 纯磁盘查询（快照模式）
   */
  private queryFromDisk(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    const isFullScan = this.isFullScanQuery(criteria);

    if (isFullScan) {
      return this.executeFullScanFromDisk();
    }

    return this.executeIndexedQueryFromDisk(criteria);
  }

  /**
   * 内存+磁盘流式查询
   */
  private async *streamFromMemoryAndDisk(
    criteria: Partial<EncodedTriple>,
    batchSize: number,
  ): AsyncGenerator<EncodedTriple[], void, unknown> {
    const isFullScan = this.isFullScanQuery(criteria);

    if (isFullScan) {
      yield* this.streamFullScan(batchSize);
    } else {
      yield* this.streamIndexedQuery(criteria, batchSize);
    }
  }

  /**
   * 纯磁盘流式查询
   */
  private async *streamFromDisk(
    criteria: Partial<EncodedTriple>,
    batchSize: number,
  ): AsyncGenerator<EncodedTriple[], void, unknown> {
    const isFullScan = this.isFullScanQuery(criteria);

    if (isFullScan) {
      yield* this.streamFullScanFromDisk(batchSize);
    } else {
      for (const batch of this.streamIndexedQueryFromDisk(criteria, batchSize)) {
        yield batch;
      }
    }
  }

  /**
   * 检查是否为全量扫描查询
   */
  private isFullScanQuery(criteria: Partial<EncodedTriple>): boolean {
    return (
      criteria.subjectId === undefined &&
      criteria.predicateId === undefined &&
      criteria.objectId === undefined
    );
  }

  /**
   * 执行全量扫描（内存+磁盘）
   */
  private executeFullScan(): EncodedTriple[] {
    const spoReader = this.context.pagedIndex.getReader('SPO');
    if (!spoReader) {
      return this.filterMemoryTriples(this.context.getMemoryTriples(), {});
    }

    const allTriples = new Set<string>();
    const results: EncodedTriple[] = [];

    try {
      const primaryValues = new Set(spoReader.getPrimaryValues());
      for (const primaryValue of primaryValues) {
        const triples = spoReader.readSync(primaryValue);
        for (const t of triples) {
          const key = encodeTripleKey(t);
          if (!allTriples.has(key) && !this.context.tombstones.has(key)) {
            allTriples.add(key);
            results.push(t);
          }
        }
      }

      // 合并内存增量
      for (const t of this.context.getMemoryTriples()) {
        const key = encodeTripleKey(t);
        if (!allTriples.has(key) && !this.context.tombstones.has(key)) {
          results.push(t);
        }
      }

      return results;
    } catch {
      return this.filterMemoryTriples(this.context.getMemoryTriples(), {});
    }
  }

  /**
   * 执行索引查询（内存+磁盘）
   */
  private executeIndexedQuery(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    const order = getBestIndexKey(criteria);
    const reader = this.context.pagedIndex.getReader(order);
    const primaryValue = criteria[primaryKey(order)];

    if (reader && primaryValue !== undefined) {
      // 记录热度统计
      this.context.bumpHot?.(order, primaryValue);

      const pagedTriples = reader.readSync(primaryValue);
      const pagedResults = pagedTriples.filter(
        (t) => matchCriteria(t, criteria) && !this.context.tombstones.has(encodeTripleKey(t)),
      );

      const memResults = this.filterMemoryTriples(this.context.getMemoryTriples(), criteria);
      return this.deduplicateTriples([...pagedResults, ...memResults]);
    }

    return this.filterMemoryTriples(this.context.getMemoryTriples(), criteria);
  }

  /**
   * 执行全量扫描（纯磁盘）
   */
  private executeFullScanFromDisk(): EncodedTriple[] {
    const spoReader = this.context.pagedIndex.getReader('SPO');
    if (!spoReader) return [];

    const results: EncodedTriple[] = [];
    const seen = new Set<string>();

    try {
      const primaryValuesArr = spoReader.getPrimaryValues();
      for (const primaryValue of primaryValuesArr) {
        const triples = spoReader.readSync(primaryValue);
        for (const t of triples) {
          const key = encodeTripleKey(t);
          if (!seen.has(key) && !this.context.tombstones.has(key)) {
            seen.add(key);
            results.push(t);
          }
        }
      }
    } catch {
      // 磁盘读取失败
    }

    return results;
  }

  /**
   * 执行索引查询（纯磁盘）
   */
  private executeIndexedQueryFromDisk(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    const order = getBestIndexKey(criteria);
    const reader = this.context.pagedIndex.getReader(order);
    const primaryValue = criteria[primaryKey(order)];

    if (reader && primaryValue !== undefined) {
      try {
        const pagedTriples = reader.readSync(primaryValue);
        return pagedTriples.filter(
          (t) => matchCriteria(t, criteria) && !this.context.tombstones.has(encodeTripleKey(t)),
        );
      } catch {
        // 磁盘读取失败
      }
    }

    return [];
  }

  /**
   * 流式全量扫描
   */
  private async *streamFullScan(batchSize: number): AsyncGenerator<EncodedTriple[], void, unknown> {
    const spoReader = this.context.pagedIndex.getReader('SPO');
    if (!spoReader) {
      yield* this.batchArray(
        this.filterMemoryTriples(this.context.getMemoryTriples(), {}),
        batchSize,
      );
      return;
    }

    const seenKeys = new Set<string>();
    let batch: EncodedTriple[] = [];

    // 流式读取分页索引数据
    for await (const pageTriples of spoReader.streamAll()) {
      for (const t of pageTriples) {
        const key = encodeTripleKey(t);
        if (!seenKeys.has(key) && !this.context.tombstones.has(key)) {
          seenKeys.add(key);
          batch.push(t);

          if (batch.length >= batchSize) {
            yield [...batch];
            batch = [];
          }
        }
      }
    }

    // 合并内存中的增量数据
    for (const t of this.context.getMemoryTriples()) {
      const key = encodeTripleKey(t);
      if (!seenKeys.has(key) && !this.context.tombstones.has(key)) {
        batch.push(t);

        if (batch.length >= batchSize) {
          yield [...batch];
          batch = [];
        }
      }
    }

    if (batch.length > 0) {
      yield batch;
    }
  }

  /**
   * 流式索引查询
   */
  private async *streamIndexedQuery(
    criteria: Partial<EncodedTriple>,
    batchSize: number,
  ): AsyncGenerator<EncodedTriple[], void, unknown> {
    const order = getBestIndexKey(criteria);
    const reader = this.context.pagedIndex.getReader(order);
    const primaryValue = criteria[primaryKey(order)];

    if (reader && primaryValue !== undefined) {
      const seenKeys = new Set<string>();
      let batch: EncodedTriple[] = [];

      // 流式读取分页索引数据
      for await (const pageTriples of reader.streamByPrimaryValue(primaryValue)) {
        for (const t of pageTriples) {
          if (matchCriteria(t, criteria) && !this.context.tombstones.has(encodeTripleKey(t))) {
            const key = encodeTripleKey(t);
            if (!seenKeys.has(key)) {
              seenKeys.add(key);
              batch.push(t);

              if (batch.length >= batchSize) {
                yield [...batch];
                batch = [];
              }
            }
          }
        }
      }

      // 合并内存中的增量数据
      for (const t of this.context.getMemoryTriples()) {
        if (matchCriteria(t, criteria) && !this.context.tombstones.has(encodeTripleKey(t))) {
          const key = encodeTripleKey(t);
          if (!seenKeys.has(key)) {
            batch.push(t);

            if (batch.length >= batchSize) {
              yield [...batch];
              batch = [];
            }
          }
        }
      }

      if (batch.length > 0) {
        yield batch;
      }
      return;
    }

    // 回退：内存数据的分批处理
    const memTriples = this.filterMemoryTriples(this.context.getMemoryTriples(), criteria);
    yield* this.batchArray(memTriples, batchSize);
  }

  /**
   * 流式全量扫描（纯磁盘）
   */
  private async *streamFullScanFromDisk(
    batchSize: number,
  ): AsyncGenerator<EncodedTriple[], void, unknown> {
    const spoReader = this.context.pagedIndex.getReader('SPO');
    if (!spoReader) return;

    const seen = new Set<string>();
    let batch: EncodedTriple[] = [];

    try {
      for await (const triple of spoReader.readAllStreaming()) {
        if (this.context.tombstones.has(encodeTripleKey(triple))) continue;
        const key = encodeTripleKey(triple);
        if (seen.has(key)) continue;
        seen.add(key);

        batch.push(triple);
        if (batch.length >= batchSize) {
          yield [...batch];
          batch = [];
        }
      }

      if (batch.length > 0) {
        yield batch;
      }
    } catch {
      // 读取失败时不产生任何结果
    }
  }

  /**
   * 流式索引查询（纯磁盘）
   */
  private *streamIndexedQueryFromDisk(
    criteria: Partial<EncodedTriple>,
    batchSize: number,
  ): Generator<EncodedTriple[], void, unknown> {
    const order = getBestIndexKey(criteria);
    const reader = this.context.pagedIndex.getReader(order);
    const primaryValue = criteria[primaryKey(order)];

    if (reader && primaryValue !== undefined) {
      let batch: EncodedTriple[] = [];

      try {
        const pagedTriples = reader.readSync(primaryValue);
        for (const t of pagedTriples) {
          if (matchCriteria(t, criteria) && !this.context.tombstones.has(encodeTripleKey(t))) {
            batch.push(t);
            if (batch.length >= batchSize) {
              yield [...batch];
              batch = [];
            }
          }
        }

        if (batch.length > 0) {
          yield batch;
        }
      } catch {
        // 读取失败时不产生任何结果
      }
    }
  }

  /**
   * 过滤内存三元组
   */
  private filterMemoryTriples(
    triples: EncodedTriple[],
    criteria: Partial<EncodedTriple>,
  ): EncodedTriple[] {
    return triples.filter(
      (t) => matchCriteria(t, criteria) && !this.context.tombstones.has(encodeTripleKey(t)),
    );
  }

  /**
   * 去重三元组
   */
  private deduplicateTriples(triples: EncodedTriple[]): EncodedTriple[] {
    const seen = new Set<string>();
    const results: EncodedTriple[] = [];

    for (const t of triples) {
      const key = encodeTripleKey(t);
      if (!seen.has(key)) {
        seen.add(key);
        results.push(t);
      }
    }

    return results;
  }

  /**
   * 数组分批处理
   */
  private *batchArray<T>(array: T[], batchSize: number): Generator<T[], void, unknown> {
    for (let i = 0; i < array.length; i += batchSize) {
      yield array.slice(i, i + batchSize);
    }
  }

  /**
   * 转换为记录格式
   */
  private toFactRecord(triple: EncodedTriple, includeProps: boolean): FactRecord {
    const tripleKey = {
      subjectId: triple.subjectId,
      predicateId: triple.predicateId,
      objectId: triple.objectId,
    };

    // 架构重构（Issue #7）：优先从PropertyIndexManager读取属性
    const getNodeProps = (nodeId: number): Record<string, unknown> | undefined => {
      if (!includeProps) return undefined;

      // 1. 先从PropertyStore（增量缓存）读取
      const fromStore = this.context.properties.getNodeProperties(nodeId);
      if (fromStore !== undefined) return fromStore;

      // 2. 从PropertyIndexManager读取（磁盘数据）
      return this.context.propertyIndexManager?.getNodePropertiesSync(nodeId);
    };

    const getEdgeProps = (): Record<string, unknown> | undefined => {
      if (!includeProps) return undefined;

      // 1. 先从PropertyStore读取
      const fromStore = this.context.properties.getEdgeProperties(tripleKey);
      if (fromStore !== undefined) return fromStore;

      // 2. 从PropertyIndexManager读取
      return this.context.propertyIndexManager?.getEdgePropertiesSync(tripleKey);
    };

    return {
      subject: this.context.dictionary.getValue(triple.subjectId) ?? '',
      predicate: this.context.dictionary.getValue(triple.predicateId) ?? '',
      object: this.context.dictionary.getValue(triple.objectId) ?? '',
      subjectId: triple.subjectId,
      predicateId: triple.predicateId,
      objectId: triple.objectId,
      subjectProperties: getNodeProps(triple.subjectId),
      objectProperties: getNodeProps(triple.objectId),
      edgeProperties: getEdgeProps(),
    };
  }

  /**
   * 刷新读者（如果需要）
   */
  private async refreshReadersIfNeeded(): Promise<void> {
    try {
      const manifest = await this.context.pagedIndex.loadManifest(this.context.tombstones);
      if (!manifest) return;

      const epoch = this.context.pagedIndex.getCurrentEpoch();
      if (epoch > this.context.concurrency.getCurrentEpoch()) {
        this.context.concurrency.setCurrentEpoch(epoch);
      }
    } catch {
      // 忽略错误
    }
  }
}

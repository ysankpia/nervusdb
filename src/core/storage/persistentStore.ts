import { promises as fsp } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { initializeIfMissing, readStorageFile } from './fileHeader.js';
import {
  loadNativeCore,
  type NativeDatabaseHandle,
  type NativeQueryCriteria,
  type NativeTriple,
} from '../../native/core.js';
import { StringDictionary } from './dictionary.js';
import { PropertyStore, TripleKey } from './propertyStore.js';
import { TripleIndexes, type IndexOrder } from './tripleIndexes.js';
import { PropertyIndexManager, type PropertyChange } from './propertyIndex.js';
import { LabelManager } from '../../graph/labels.js';
import { EncodedTriple, TripleStore } from './tripleStore.js';
import { LsmLiteStaging } from './staging.js';
import { readHotness, type HotnessData } from './hotness.js';
import { cleanupProcessReaders } from './readerRegistry.js';
import { DEFAULT_PAGE_SIZE } from './pagedIndex.js';
import { PagedIndexCoordinator } from './managers/pagedIndexCoordinator.js';
import { WalManager } from './managers/walManager.js';
import { TransactionManager, type TransactionBatch } from './managers/transactionManager.js';
import { ConcurrencyControl } from './managers/concurrencyControl.js';
import { QueryEngine, type QueryContext } from './managers/queryEngine.js';
import { FlushManager, type FlushContext } from './managers/flushManager.js';
import { encodeTripleKey, decodeTripleKey } from './helpers/tripleOrdering.js';
import type { FactInput } from './types.js';

export interface PersistedFact extends FactInput {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

export interface FactRecord extends PersistedFact {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

export interface PersistentStoreOptions {
  indexDirectory?: string;
  pageSize?: number;
  rebuildIndexes?: boolean;
  compression?: {
    codec: 'none' | 'brotli';
    level?: number;
  };
  enableLock?: boolean; // 启用进程级独占写锁（同一路径只允许一个写者）
  registerReader?: boolean; // 打开时注册为读者（跨进程可见）
  enablePersistentTxDedupe?: boolean; // 启用跨周期 txId 幂等去重
  maxRememberTxIds?: number; // 记忆的最大 txId 数（默认 1000）
  stagingMode?: 'default' | 'lsm-lite'; // 预留写入策略（当前仅接收参数，不改变行为）
}

export type { FactInput } from './types.js';

export class PersistentStore {
  private constructor(
    private readonly path: string,
    private readonly dictionary: StringDictionary,
    private readonly triples: TripleStore,
    private readonly properties: PropertyStore,
    private readonly indexes: TripleIndexes,
    private readonly indexDirectory: string,
  ) {}

  private dirty = false;
  private wal!: WalManager;
  private transactionManager!: TransactionManager;
  private concurrencyControl!: ConcurrencyControl;
  private queryEngine!: QueryEngine;
  private flushManager!: FlushManager;
  private closed = false;
  private tombstones = new Set<string>();
  private hotness: HotnessData | null = null;
  private propertyIndexManager!: PropertyIndexManager;
  private labelManager!: LabelManager;
  private lsm?: LsmLiteStaging<EncodedTriple>;
  private nativeHandle?: NativeDatabaseHandle;
  private nativeQueryReady = false;
  private nativeStrict = process.env.NERVUSDB_NATIVE_STRICT === '1';
  // 内存模式（:memory:）支持：使用临时文件路径并在关闭时清理
  private memoryMode = false;
  private memoryBasePath?: string;

  static async open(path: string, options: PersistentStoreOptions = {}): Promise<PersistentStore> {
    // 为 ':memory:' 提供真正的内存数据库语义：映射到唯一的临时路径
    let effectivePath = path;
    let memoryMode = false;
    if (path === ':memory:') {
      memoryMode = true;
      const unique = `synapsedb-memory-${process.pid}-${Date.now()}-${Math.random()
        .toString(36)
        .slice(2)}`;
      effectivePath = join(tmpdir(), `${unique}.synapsedb`);
    }

    await initializeIfMissing(effectivePath);

    let nativeHandle: NativeDatabaseHandle | undefined;
    const nativeBinding = loadNativeCore();
    if (nativeBinding) {
      try {
        nativeHandle = nativeBinding.open({ dataPath: effectivePath });
      } catch (error) {
        if (process.env.NERVUSDB_NATIVE_STRICT === '1') {
          throw error;
        }
      }
    }
    // 当存在写锁且尝试以无锁模式打开时，若 WAL 非空（存在未落盘的写入），拒绝无锁访问
    // 用于防止已加锁写者运行期间，第二个“伪读者”的无锁写入引发并发风险
    try {
      if (options.enableLock === false) {
        const lockPath = `${effectivePath}.lock`;
        const walPath = `${effectivePath}.wal`;
        // 检查锁文件是否存在
        const [lstat, wstat] = await Promise.allSettled([fsp.stat(lockPath), fsp.stat(walPath)]);
        const locked = lstat.status === 'fulfilled';
        const walSize = wstat.status === 'fulfilled' ? (wstat.value.size ?? 0) : 0;
        // WAL header 固定 12 字节；大于 12 说明存在未 reset 的写入
        if (locked && walSize > 12) {
          throw new Error(
            '数据库当前由写者持有锁且存在未落盘的 WAL 写入，禁止无锁打开。请等待写者 flush/释放后再以读者模式访问。',
          );
        }
      }
    } catch {
      // 防御性：出现异常时不影响正常打开流程
    }
    const sections = await readStorageFile(effectivePath);
    const dictionary = StringDictionary.deserialize(sections.dictionary);
    // 架构重构：不再加载完整TripleStore到内存，改为仅加载增量数据
    // 历史数据通过分页索引访问，只有WAL重放数据加载到内存
    const triples = new TripleStore(); // 创建空的TripleStore，仅用于WAL重放和新增数据

    // 架构重构（阶段一-Issue #7）：PropertyStore 迁移策略
    // 1. 从主文件反序列化属性数据（用于数据迁移）
    // 2. 后续将迁移到 PropertyDataStore 的分页存储
    const propertyStoreFromFile = PropertyStore.deserialize(sections.properties);
    const propertyStore = new PropertyStore(); // 创建空实例，仅用于增量缓存

    const indexes = TripleIndexes.deserialize(sections.indexes);
    // 初次打开且无 manifest 时，将以全量方式重建分页索引，无需在内存中保有全部索引
    const indexDirectory = options.indexDirectory ?? `${effectivePath}.pages`;

    // 清理当前进程可能残留的旧reader文件（防止上次异常退出的残留）
    try {
      await cleanupProcessReaders(indexDirectory, process.pid);
    } catch {
      // 忽略清理错误，不影响数据库打开
    }

    const store = new PersistentStore(
      effectivePath,
      dictionary,
      triples,
      propertyStore,
      indexes,
      indexDirectory,
    );
    // 标记内存模式并记录基础路径，供 close() 清理
    store.memoryMode = memoryMode;
    store.memoryBasePath = effectivePath;
    store.nativeHandle = nativeHandle;

    // 初始化并发控制管理器（需要在锁操作之前初始化）
    store.concurrencyControl = new ConcurrencyControl(indexDirectory, effectivePath);

    if (options.enableLock) {
      await store.concurrencyControl.acquireWriteLock();
    }
    if (options.stagingMode === 'lsm-lite') {
      store.lsm = new LsmLiteStaging<EncodedTriple>();
    }

    // 初始化属性索引管理器
    store.propertyIndexManager = new PropertyIndexManager(indexDirectory);
    await store.propertyIndexManager.initialize();

    // 数据迁移：将主文件中的属性数据迁移到 PropertyDataStore
    // 这一步在首次打开时执行，后续打开将直接从 PropertyDataStore 加载
    const nodePropsToMigrate = propertyStoreFromFile.getAllNodeProperties();
    const edgePropsToMigrate = propertyStoreFromFile.getAllEdgeProperties();

    // 迁移节点属性到 PropertyDataStore
    for (const [nodeId, properties] of nodePropsToMigrate.entries()) {
      store.propertyIndexManager.setNodeProperties(nodeId, properties);
    }

    // 迁移边属性到 PropertyDataStore
    for (const [edgeKey, properties] of edgePropsToMigrate.entries()) {
      const [subjectId, predicateId, objectId] = edgeKey.split(':').map(Number);
      store.propertyIndexManager.setEdgeProperties(
        { subjectId, predicateId, objectId },
        properties,
      );
    }

    // 初始化标签管理器
    store.labelManager = new LabelManager(indexDirectory);

    // 初始化分页索引协调器
    store.pagedIndex = new PagedIndexCoordinator({ indexDirectory });

    // WAL 重放（将未持久化的增量恢复到内存与 staging）
    const { manager: walManager, replay } = await WalManager.initialize(effectivePath, {
      indexDirectory,
      enablePersistentTxDedupe: options.enablePersistentTxDedupe,
      maxRememberTxIds: options.maxRememberTxIds,
    });
    store.wal = walManager;

    // 初始化事务管理器
    store.transactionManager = new TransactionManager(walManager);

    for (const f of replay.addFacts) store.addFactDirect(f);
    for (const f of replay.deleteFacts) store.deleteFactDirect(f);
    for (const n of replay.nodeProps)
      store.setNodePropertiesDirect(n.nodeId, n.value as Record<string, unknown>);
    for (const e of replay.edgeProps)
      store.setEdgePropertiesDirect(e.ids, e.value as Record<string, unknown>);
    const manifest = await store.pagedIndex.loadManifest(store.tombstones);
    const desiredPageSize = options.pageSize ?? DEFAULT_PAGE_SIZE;
    const shouldRebuild =
      options.rebuildIndexes === true || !manifest || manifest.pageSize !== desiredPageSize;

    if (shouldRebuild) {
      await store.pagedIndex.rebuildFromStorage(effectivePath, store.tombstones, {
        pageSize: options.pageSize,
        compression: options.compression,
      });
    }
    store.concurrencyControl.setCurrentEpoch(store.pagedIndex.getCurrentEpoch());

    // 重建属性索引
    await store.rebuildPropertyIndex();
    // 加载热度计数
    try {
      store.hotness = await readHotness(indexDirectory);
    } catch {
      store.hotness = {
        version: 1,
        updatedAt: Date.now(),
        counts: { SPO: {}, SOP: {}, POS: {}, PSO: {}, OSP: {}, OPS: {} },
      } as HotnessData;
    }

    // 初始化查询引擎（在所有其他组件就绪后）
    const queryContext: QueryContext = {
      dictionary: store.dictionary,
      properties: store.properties,
      pagedIndex: store.pagedIndex,
      concurrency: store.concurrencyControl,
      getMemoryTriples: () => store.triples.list(),
      tombstones: store.tombstones,
      bumpHot: (order, primary) => store.bumpHot(order, primary),
      // 架构重构（Issue #7）：传入propertyIndexManager用于属性查询
      propertyIndexManager: store.propertyIndexManager,
    };
    store.queryEngine = new QueryEngine(queryContext);

    // 初始化刷新管理器
    store.flushManager = new FlushManager();

    store.bootstrapNativeState();

    if (options.registerReader !== false) {
      await store.concurrencyControl.registerReader(store.concurrencyControl.getCurrentEpoch());
    }
    return store;
  }

  private pagedIndex!: PagedIndexCoordinator;

  addFact(fact: FactInput): PersistedFact {
    if (this.nativeHandle) {
      try {
        this.nativeHandle.addFact(fact.subject, fact.predicate, fact.object);
      } catch (error) {
        if (process.env.NERVUSDB_NATIVE_STRICT === '1') {
          throw error;
        }
      }
    }

    // 仅写 WAL 记录；若处于批次中，则暂存到事务管理器，最外层 commit 时再落入主存
    const inBatch = this.transactionManager.isInBatch();
    void this.wal.appendAddTriple(fact);
    const subjectId = this.dictionary.getOrCreateId(fact.subject);
    const predicateId = this.dictionary.getOrCreateId(fact.predicate);
    const objectId = this.dictionary.getOrCreateId(fact.object);

    const triple: EncodedTriple = {
      subjectId,
      predicateId,
      objectId,
    };
    if (inBatch) {
      // 暂存到事务管理器，不立即变更主存
      this.transactionManager.addTripleToCurrentBatch(triple);
    } else {
      if (!this.triples.has(triple)) {
        this.triples.add(triple);
        this.stageAdd(triple);
        this.dirty = true;
      }
    }

    return {
      ...fact,
      subjectId,
      predicateId,
      objectId,
    };
  }

  private addFactDirect(fact: FactInput): PersistedFact {
    const subjectId = this.dictionary.getOrCreateId(fact.subject);
    const predicateId = this.dictionary.getOrCreateId(fact.predicate);
    const objectId = this.dictionary.getOrCreateId(fact.object);

    const triple: EncodedTriple = {
      subjectId,
      predicateId,
      objectId,
    };

    if (!this.triples.has(triple)) {
      this.triples.add(triple);
      this.stageAdd(triple);
      this.dirty = true;
    } else {
      // 已存在于主文件：为了查询可见性，仍将其加入暂存索引并标记脏，直到下一次 flush 合并分页
      this.stageAdd(triple);
      this.dirty = true;
    }

    return {
      ...fact,
      subjectId,
      predicateId,
      objectId,
    };
  }

  listFacts(): FactRecord[] {
    // 架构重构：优先从分页索引读取全部数据，合并内存中的增量数据
    const allTriples = this.query({}); // 使用重构后的query方法获取所有数据
    return this.resolveRecords(allTriples);
  }

  // 流式查询：逐批返回查询结果，避免大结果集内存压力
  async *streamQuery(
    criteria: Partial<EncodedTriple>,
    batchSize = 1000,
  ): AsyncGenerator<EncodedTriple[], void, unknown> {
    yield* this.queryEngine.streamQuery(criteria, { batchSize });
  }

  // 流式查询记录：返回解析后的FactRecord批次
  async *streamFactRecords(
    criteria: Partial<EncodedTriple> = {},
    batchSize = 1000,
    options: { includeProperties?: boolean } = {},
  ): AsyncGenerator<FactRecord[], void, unknown> {
    yield* this.queryEngine.streamFactRecords(criteria, {
      batchSize,
      includeProperties: options.includeProperties,
    });
  }

  getDictionarySize(): number {
    return this.dictionary.size;
  }

  /**
   * 获取分页索引的 manifest（只读，用于诊断/估算）
   */
  getIndexManifest() {
    return this.pagedIndex.getManifest();
  }

  /**
   * 获取热度数据快照（只读，用于诊断/估算）
   */
  getHotnessSnapshot() {
    return this.hotness;
  }

  hasPagedIndexData(order: IndexOrder = 'SPO'): boolean {
    const reader = this.pagedIndex.getReader(order);
    if (!reader) return false;
    try {
      return reader.getPrimaryValues().length > 0;
    } catch {
      return false;
    }
  }

  getNodeIdByValue(value: string): number | undefined {
    return this.dictionary.getId(value);
  }

  getNodeValueById(id: number): string | undefined {
    return this.dictionary.getValue(id);
  }

  deleteFact(fact: FactInput): void {
    const inBatch = this.transactionManager.isInBatch();
    void this.wal.appendDeleteTriple(fact);
    if (inBatch) {
      const subjectId = this.dictionary.getOrCreateId(fact.subject);
      const predicateId = this.dictionary.getOrCreateId(fact.predicate);
      const objectId = this.dictionary.getOrCreateId(fact.object);
      const triple: EncodedTriple = { subjectId, predicateId, objectId };
      this.transactionManager.deleteTripleFromCurrentBatch(triple);
    } else {
      this.deleteFactDirect(fact);
    }
  }

  private deleteFactDirect(fact: FactInput): void {
    const subjectId = this.dictionary.getOrCreateId(fact.subject);
    const predicateId = this.dictionary.getOrCreateId(fact.predicate);
    const objectId = this.dictionary.getOrCreateId(fact.object);
    this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
    this.dirty = true;
  }

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    const inBatch = this.transactionManager.isInBatch();

    // 获取旧属性用于索引更新
    const oldProperties = this.getNodeProperties(nodeId) || {};

    void this.wal.appendSetNodeProps(nodeId, properties);
    if (inBatch) {
      this.transactionManager.setNodePropertiesInCurrentBatch(nodeId, properties);
    } else {
      this.properties.setNodeProperties(nodeId, properties);
      this.dirty = true;

      // 架构重构（Issue #7）：同时更新 PropertyDataStore 缓存
      this.propertyIndexManager.setNodeProperties(nodeId, properties);

      // 更新属性索引
      this.updateNodePropertyIndex(nodeId, oldProperties, properties);

      // 更新标签索引
      this.updateNodeLabelIndex(nodeId, oldProperties, properties);
    }
  }

  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    const inBatch = this.transactionManager.isInBatch();

    // 获取旧属性用于索引更新
    const oldProperties = this.getEdgeProperties(key) || {};
    const edgeKey = encodeTripleKey(key);

    void this.wal.appendSetEdgeProps(key, properties);
    if (inBatch) {
      this.transactionManager.setEdgePropertiesInCurrentBatch(edgeKey, properties);
    } else {
      this.properties.setEdgeProperties(key, properties);
      this.dirty = true;

      // 架构重构（Issue #7）：同时更新 PropertyDataStore 缓存
      this.propertyIndexManager.setEdgeProperties(key, properties);

      // 更新属性索引
      this.updateEdgePropertyIndex(edgeKey, oldProperties, properties);
    }
  }

  // 事务批次（可选）：外部可将多条写入合并为一个 WAL 批次
  beginBatch(options?: { txId?: string; sessionId?: string }): void {
    this.transactionManager.beginBatch(options);
  }

  commitBatch(options?: { durable?: boolean }): void {
    const stage = this.transactionManager.commitBatch(options);

    if (stage) {
      if (this.transactionManager.getBatchDepth() === 0) {
        // 最外层提交：将暂存应用到主存
        this.applyStage(stage);
      } else {
        // 内层提交：立即应用到主存，使其不受外层 ABORT 影响（与测试语义一致）
        this.applyStage(stage);
      }
    }
  }

  abortBatch(): void {
    this.transactionManager.abortBatch();
  }

  private setNodePropertiesDirect(nodeId: number, properties: Record<string, unknown>): void {
    // 获取旧属性用于索引更新（WAL 重放场景）
    const oldProperties = this.properties.getNodeProperties(nodeId) || {};

    this.properties.setNodeProperties(nodeId, properties);
    this.dirty = true;

    // 架构重构（Issue #7）：同时更新 PropertyDataStore 缓存
    this.propertyIndexManager.setNodeProperties(nodeId, properties);

    // 更新属性索引
    this.updateNodePropertyIndex(nodeId, oldProperties, properties);
  }

  private setEdgePropertiesDirect(key: TripleKey, properties: Record<string, unknown>): void {
    // 获取旧属性用于索引更新（WAL 重放场景）
    const oldProperties = this.properties.getEdgeProperties(key) || {};
    const edgeKey = encodeTripleKey(key);

    this.properties.setEdgeProperties(key, properties);
    this.dirty = true;

    // 架构重构（Issue #7）：同时更新 PropertyDataStore 缓存
    this.propertyIndexManager.setEdgeProperties(key, properties);

    // 更新属性索引
    this.updateEdgePropertyIndex(edgeKey, oldProperties, properties);
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | undefined {
    // 若处于事务中，优先返回事务暂存视图
    const txValue = this.transactionManager.getNodePropertiesFromTransaction(nodeId);
    if (txValue !== undefined) return txValue;

    // 架构重构（Issue #7）：从 PropertyIndexManager 读取（缓存优先）
    // 1. 首先尝试从增量缓存（PropertyStore）读取
    const fromCache = this.properties.getNodeProperties(nodeId);
    if (fromCache !== undefined) return fromCache;

    // 2. 从 PropertyDataStore 读取（已预加载到缓存）
    return this.propertyIndexManager.getNodePropertiesSync(nodeId);
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined {
    const edgeKey = encodeTripleKey(key);
    // 若处于事务中，优先返回事务暂存视图
    const txValue = this.transactionManager.getEdgePropertiesFromTransaction(edgeKey);
    if (txValue !== undefined) return txValue;

    // 架构重构（Issue #7）：从 PropertyIndexManager 读取
    // 1. 首先尝试从增量缓存（PropertyStore）读取
    const fromCache = this.properties.getEdgeProperties(key);
    if (fromCache !== undefined) return fromCache;

    // 2. 从 PropertyDataStore 读取
    return this.propertyIndexManager.getEdgePropertiesSync(key);
  }

  private getTriplesByPrimarySet(order: IndexOrder, primaryIds: Set<number>): EncodedTriple[] {
    if (primaryIds.size === 0) {
      return [];
    }
    const reader = this.pagedIndex.getReader(order);
    const results: EncodedTriple[] = [];
    const seen = new Set<string>();

    if (reader) {
      try {
        const triples = reader.readMany(primaryIds);
        for (const t of triples) {
          const key = encodeTripleKey(t);
          if (this.tombstones.has(key) || seen.has(key)) continue;
          seen.add(key);
          results.push(t);
        }
      } catch (err) {
        if ((err as NodeJS.ErrnoException)?.code !== 'ENOENT') {
          throw err;
        }
        // 索引文件尚未构建，回退到内存数据
      }
    }

    for (const t of this.triples.list()) {
      const primaryValue =
        order === 'SPO'
          ? t.subjectId
          : order === 'POS' || order === 'PSO'
            ? t.predicateId
            : t.objectId;
      if (!primaryIds.has(primaryValue)) continue;
      const key = encodeTripleKey(t);
      if (this.tombstones.has(key) || seen.has(key)) continue;
      seen.add(key);
      results.push(t);
    }

    return results;
  }

  query(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    const nativeResult = this.tryNativeQuery(criteria);
    if (nativeResult !== undefined) {
      return nativeResult;
    }
    return this.queryEngine.query(criteria);
  }

  /**
   * 流式查询：统一委托给QueryEngine
   */
  async *queryStreaming(criteria: Partial<EncodedTriple>): AsyncIterableIterator<EncodedTriple> {
    const nativeResult = this.tryNativeQuery(criteria);
    if (nativeResult !== undefined) {
      for (const triple of nativeResult) {
        yield triple;
      }
      return;
    }

    for await (const batch of this.queryEngine.streamQuery(criteria)) {
      yield* batch;
    }
  }

  resolveRecords(
    triples: EncodedTriple[],
    options?: { includeProperties?: boolean },
  ): FactRecord[] {
    return this.queryEngine.resolveRecords(triples, options);
  }

  private bootstrapNativeState(): void {
    if (!this.nativeHandle || typeof this.nativeHandle.hydrate !== 'function') {
      this.nativeQueryReady = false;
      return;
    }

    const dictionaryValues = this.dictionary.getValuesSnapshot();

    try {
      const triples = this.queryEngine.query({});
      const payload: NativeTriple[] = triples.map((triple) => ({
        subject_id: triple.subjectId,
        predicate_id: triple.predicateId,
        object_id: triple.objectId,
      }));
      this.nativeHandle.hydrate(dictionaryValues, payload);
      this.nativeQueryReady = true;
    } catch (error) {
      this.nativeQueryReady = false;
      if (this.nativeStrict) {
        throw error;
      }
    }
  }

  private tryNativeQuery(criteria: Partial<EncodedTriple>): EncodedTriple[] | undefined {
    if (
      !this.nativeHandle ||
      !this.nativeQueryReady ||
      typeof this.nativeHandle.query !== 'function'
    ) {
      return undefined;
    }

    const nativeCriteria: NativeQueryCriteria = {};
    if (criteria.subjectId !== undefined) nativeCriteria.subject_id = criteria.subjectId;
    if (criteria.predicateId !== undefined) nativeCriteria.predicate_id = criteria.predicateId;
    if (criteria.objectId !== undefined) nativeCriteria.object_id = criteria.objectId;

    try {
      const result = this.nativeHandle.query(
        Object.keys(nativeCriteria).length > 0 ? nativeCriteria : undefined,
      );
      return result.map((triple) => ({
        subjectId: triple.subject_id,
        predicateId: triple.predicate_id,
        objectId: triple.object_id,
      }));
    } catch (error) {
      if (this.nativeStrict) {
        throw error;
      }
      return undefined;
    }
  }

  async flush(): Promise<void> {
    if (this.closed) return;

    // 使用新的增量刷新管理器：消除O(N)写放大
    const context: FlushContext = {
      path: this.path,
      indexDirectory: this.indexDirectory,
      dictionary: this.dictionary,
      triples: this.triples,
      properties: this.properties,
      indexes: this.indexes,
      pagedIndex: this.pagedIndex,
      propertyIndexManager: this.propertyIndexManager,
      wal: this.wal,
      concurrency: this.concurrencyControl,
      tombstones: this.tombstones,
      hotness: this.hotness,
      lsm: this.lsm,
    };

    await this.flushManager.flush(context, this.dirty);
    this.dirty = false;
  }

  // 读一致性：在查询链路中临时固定 epoch，避免中途重载 readers
  async pushPinnedEpoch(epoch: number): Promise<void> {
    await this.concurrencyControl.pushPinnedEpoch(epoch);
  }

  async popPinnedEpoch(): Promise<void> {
    await this.concurrencyControl.popPinnedEpoch();
  }

  getCurrentEpoch(): number {
    return this.concurrencyControl.getCurrentEpoch();
  }

  // 暂存层指标（仅用于观测与基准）
  getStagingMetrics(): { lsmMemtable: number } {
    return { lsmMemtable: this.lsm ? this.lsm.size() : 0 };
  }

  async close(): Promise<void> {
    if (this.nativeHandle) {
      try {
        this.nativeHandle.close();
      } catch {
        if (process.env.NERVUSDB_NATIVE_STRICT === '1') {
          throw new Error('native database close failed');
        }
      }
      this.nativeHandle = undefined;
      this.nativeQueryReady = false;
    }

    // 如果存在未持久化的数据，优先刷新到磁盘，避免依赖重放
    try {
      if (this.dirty) {
        await this.flush();
      }
    } catch {
      // 刷新失败不阻断关闭流程（测试环境容忍）
    }
    this.closed = true;
    // 关闭 WAL 句柄，避免 FileHandle 依赖 GC 关闭导致的 DEP0137 警告
    try {
      await this.wal.close();
    } catch {
      // 忽略关闭失败
    }
    // 清理并发控制（包括锁和读者注册）
    await this.concurrencyControl.cleanup();

    // 清理内存结构，避免内存泄漏
    this.tombstones.clear();
    this.transactionManager.clear();
    this.hotness = null;

    // 清理 LSM memtable
    if (this.lsm) {
      this.lsm.drain(); // 清空 memtable
      this.lsm = undefined;
    }

    // 若为内存模式（:memory:），清理临时文件与目录
    if (this.memoryMode && this.memoryBasePath) {
      try {
        await fsp.rm(`${this.memoryBasePath}.pages`, { recursive: true, force: true });
      } catch {}
      try {
        await fsp.unlink(`${this.memoryBasePath}.wal`).catch(() => {});
      } catch {}
      try {
        await fsp.unlink(`${this.memoryBasePath}.lock`).catch(() => {});
      } catch {}
      try {
        await fsp.unlink(this.memoryBasePath).catch(() => {});
      } catch {}
    }
  }

  private bumpHot(order: IndexOrder, primary: number): void {
    if (!this.hotness) return;
    const counts = this.hotness.counts;
    const bucket = counts[order] ?? {};
    const key = String(primary);
    bucket[key] = (bucket[key] ?? 0) + 1;
    counts[order] = bucket;
  }

  // 统一暂存写入：默认写入 TripleIndexes；在 lsm-lite 模式下旁路收集 memtable（不改变可见性）
  private stageAdd(t: EncodedTriple): void {
    this.indexes.add(t);
    if (this.lsm) this.lsm.add(t);
  }

  private applyStage(stage: TransactionBatch): void {
    // 应用新增
    for (const t of stage.adds) {
      if (!this.triples.has(t)) this.triples.add(t);
      // 为查询可见性，新增统一进入暂存索引，待下一次 flush 合并分页索引
      this.stageAdd(t);
      this.dirty = true;
    }
    // 应用删除
    for (const t of stage.dels) {
      this.tombstones.add(encodeTripleKey(t));
      this.dirty = true;
    }
    // 应用属性（在事务提交时更新属性索引）
    stage.nodeProps.forEach((newProperties, nodeId) => {
      // 获取旧属性用于索引更新
      const oldProperties = this.properties.getNodeProperties(nodeId) || {};
      this.properties.setNodeProperties(nodeId, newProperties);
      this.dirty = true;

      // 架构重构（Issue #7）：同时更新 PropertyDataStore 缓存
      this.propertyIndexManager.setNodeProperties(nodeId, newProperties);

      // 更新属性索引
      this.updateNodePropertyIndex(nodeId, oldProperties, newProperties);
    });
    stage.edgeProps.forEach((newProperties, edgeKey) => {
      const ids = decodeTripleKey(edgeKey);
      // 获取旧属性用于索引更新
      const oldProperties = this.properties.getEdgeProperties(ids) || {};
      this.properties.setEdgeProperties(ids, newProperties);
      this.dirty = true;

      // 架构重构（Issue #7）：同时更新 PropertyDataStore 缓存
      this.propertyIndexManager.setEdgeProperties(ids, newProperties);

      // 更新属性索引
      this.updateEdgePropertyIndex(edgeKey, oldProperties, newProperties);
    });
  }

  /**
   * 重建属性索引
   *
   * 架构重构（Issue #7）：优先从 PropertyIndexManager 的缓存获取数据
   */
  private async rebuildPropertyIndex(): Promise<void> {
    // 从 PropertyIndexManager 的缓存获取数据（已迁移的数据）
    const cached = this.propertyIndexManager.getAllCachedProperties();
    let nodeProperties = cached.nodeProperties;
    let edgeProperties = cached.edgeProperties;

    // 如果缓存为空，回退到从 PropertyStore 获取（向后兼容）
    if (nodeProperties.size === 0) {
      nodeProperties = this.properties.getAllNodeProperties();
      edgeProperties = this.properties.getAllEdgeProperties();
    }

    // 重建属性索引
    await this.propertyIndexManager.rebuildFromProperties(nodeProperties, edgeProperties);

    // 重建标签索引
    await this.labelManager.rebuildFromNodeProperties(nodeProperties);
  }

  /**
   * 获取属性索引管理器的内存索引
   */
  getPropertyIndex() {
    return this.propertyIndexManager.getMemoryIndex();
  }

  /**
   * 获取标签管理器的内存索引
   */
  getLabelIndex() {
    return this.labelManager.getMemoryIndex();
  }

  /**
   * 应用属性变更到索引
   */
  private applyPropertyIndexChange(change: PropertyChange): void {
    this.propertyIndexManager.applyPropertyChange(change);
  }

  /**
   * 更新节点标签索引
   */
  private updateNodeLabelIndex(
    nodeId: number,
    oldProperties: Record<string, unknown>,
    newProperties: Record<string, unknown>,
  ): void {
    const oldLabels = Array.isArray(oldProperties.labels)
      ? oldProperties.labels.filter((l) => typeof l === 'string')
      : [];
    const newLabels = Array.isArray(newProperties.labels)
      ? newProperties.labels.filter((l) => typeof l === 'string')
      : [];

    // 只有在标签实际发生变化时才更新索引
    if (JSON.stringify(oldLabels.sort()) !== JSON.stringify(newLabels.sort())) {
      this.labelManager.applyLabelChange(nodeId, oldLabels, newLabels);
    }
  }

  /**
   * 更新节点属性索引
   */
  private updateNodePropertyIndex(
    nodeId: number,
    oldProperties: Record<string, unknown>,
    newProperties: Record<string, unknown>,
  ): void {
    // 比较属性变化，生成索引更新操作
    const oldKeys = new Set(Object.keys(oldProperties));
    const newKeys = new Set(Object.keys(newProperties));
    const allKeys = new Set([...oldKeys, ...newKeys]);

    for (const propertyName of allKeys) {
      const oldValue = oldProperties[propertyName];
      const newValue = newProperties[propertyName];

      if (oldValue !== newValue) {
        this.applyPropertyIndexChange({
          operation: 'SET',
          target: 'node',
          targetId: nodeId,
          propertyName,
          oldValue,
          newValue,
        });
      }
    }
  }

  /**
   * 更新边属性索引
   */
  private updateEdgePropertyIndex(
    edgeKey: string,
    oldProperties: Record<string, unknown>,
    newProperties: Record<string, unknown>,
  ): void {
    // 比较属性变化，生成索引更新操作
    const oldKeys = new Set(Object.keys(oldProperties));
    const newKeys = new Set(Object.keys(newProperties));
    const allKeys = new Set([...oldKeys, ...newKeys]);

    for (const propertyName of allKeys) {
      const oldValue = oldProperties[propertyName];
      const newValue = newProperties[propertyName];

      if (oldValue !== newValue) {
        this.applyPropertyIndexChange({
          operation: 'SET',
          target: 'edge',
          targetId: edgeKey,
          propertyName,
          oldValue,
          newValue,
        });
      }
    }
  }
}

import { promises as fsp } from 'node:fs';
import { join } from 'node:path';

import { initializeIfMissing, readStorageFile, writeStorageFile } from './fileHeader';
import { StringDictionary } from './dictionary';
import { PropertyStore, TripleKey } from './propertyStore';
import { TripleIndexes, getBestIndexKey, type IndexOrder } from './tripleIndexes';
import { EncodedTriple, TripleStore } from './tripleStore';
import {
  PagedIndexReader,
  PagedIndexWriter,
  pageFileName,
  readPagedManifest,
  writePagedManifest,
  type PagedIndexManifest,
  type PageMeta,
  DEFAULT_PAGE_SIZE,
} from './pagedIndex';
import { WalReplayer, WalWriter } from './wal';
import { readHotness, writeHotness, type HotnessData } from './hotness';
import { addReader, removeReader } from './readerRegistry';
import { acquireLock, type LockHandle } from '../utils/lock';
import { triggerCrash } from '../utils/fault';

export interface FactInput {
  subject: string;
  predicate: string;
  object: string;
}

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
}

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
  private wal!: WalWriter;
  private tombstones = new Set<string>();
  private hotness: HotnessData | null = null;
  private lock?: LockHandle;
  private batchDepth = 0;
  private currentEpoch = 0;
  private lastManifestCheck = 0;
  private pinnedEpochStack: number[] = [];
  private readerRegistered = false;

  static async open(path: string, options: PersistentStoreOptions = {}): Promise<PersistentStore> {
    await initializeIfMissing(path);
    const sections = await readStorageFile(path);
    const dictionary = StringDictionary.deserialize(sections.dictionary);
    const triples = TripleStore.deserialize(sections.triples);
    const propertyStore = PropertyStore.deserialize(sections.properties);
    const indexes = TripleIndexes.deserialize(sections.indexes);
    // 初次打开且无 manifest 时，将以全量方式重建分页索引，无需在内存中保有全部索引
    const indexDirectory = options.indexDirectory ?? `${path}.pages`;
    const store = new PersistentStore(
      path,
      dictionary,
      triples,
      propertyStore,
      indexes,
      indexDirectory,
    );
    if (options.enableLock) {
      store.lock = await acquireLock(path);
    }
    // WAL 重放（将未持久化的增量恢复到内存与 staging）
    store.wal = await WalWriter.open(path);
    const replay = await new WalReplayer(path).replay();
    for (const f of replay.addFacts) store.addFactDirect(f);
    for (const f of replay.deleteFacts) store.deleteFactDirect(f);
    for (const n of replay.nodeProps)
      store.setNodePropertiesDirect(n.nodeId, n.value as Record<string, unknown>);
    for (const e of replay.edgeProps)
      store.setEdgePropertiesDirect(e.ids, e.value as Record<string, unknown>);
    // 截断 WAL 尾部不完整记录，确保下次打开幂等
    if (replay.safeOffset > 0) {
      await store.wal.truncateTo(replay.safeOffset);
    }
    const manifest = await readPagedManifest(indexDirectory);
    const shouldRebuild =
      options.rebuildIndexes === true ||
      !manifest ||
      manifest.pageSize !== (options.pageSize ?? DEFAULT_PAGE_SIZE);

    if (shouldRebuild) {
      await store.buildPagedIndexes(options.pageSize, options.compression);
    } else {
      store.hydratePagedReaders(manifest);
      store.currentEpoch = manifest.epoch ?? 0;
    }
    // 加载热度计数
    try {
      store.hotness = await readHotness(indexDirectory);
    } catch {
      store.hotness = { version: 1, updatedAt: Date.now(), counts: { SPO: {}, SOP: {}, POS: {}, PSO: {}, OSP: {}, OPS: {} } } as HotnessData;
    }
    if (options.registerReader) {
      await addReader(indexDirectory, { pid: process.pid, epoch: store.currentEpoch, ts: Date.now() });
      store.readerRegistered = true;
    }
    return store;
  }

  private pagedReaders = new Map<IndexOrder, PagedIndexReader>();

  private hydratePagedReaders(manifest: PagedIndexManifest): void {
    for (const lookup of manifest.lookups) {
      this.pagedReaders.set(
        lookup.order,
        new PagedIndexReader(
          { directory: this.indexDirectory, compression: manifest.compression },
          lookup,
        ),
      );
    }
    if (manifest.tombstones && manifest.tombstones.length > 0) {
      manifest.tombstones.forEach(([subjectId, predicateId, objectId]) => {
        this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
      });
    }
  }

  private async buildPagedIndexes(
    pageSize = DEFAULT_PAGE_SIZE,
    compression: { codec: 'none' | 'brotli'; level?: number } = { codec: 'none' },
  ): Promise<void> {
    await fsp.mkdir(this.indexDirectory, { recursive: true });

    const orders: IndexOrder[] = ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
    const lookups: Array<{
      order: IndexOrder;
      pages: { primaryValue: number; offset: number; length: number }[];
    }> = [];
    for (const order of orders) {
      const filePath = join(this.indexDirectory, pageFileName(order));
      try {
        await fsp.unlink(filePath);
      } catch {
        /* noop */
      }

      const writer = new PagedIndexWriter(filePath, {
        directory: this.indexDirectory,
        pageSize,
        compression,
      });
      // 初次/重建：写入“全量”三元组（当前从 TripleStore 一次性构建）
      const triples = this.triples.list();
      const getPrimary = primarySelector(order);
      for (const t of triples) {
        writer.push(t, getPrimary(t));
      }
      const pages = await writer.finalize();
      this.pagedReaders.set(
        order,
        new PagedIndexReader({ directory: this.indexDirectory, compression }, { order, pages }),
      );
      lookups.push({ order, pages });
    }

    const manifest: PagedIndexManifest = {
      version: 1,
      pageSize,
      createdAt: Date.now(),
      compression,
      lookups,
    };
    await writePagedManifest(this.indexDirectory, manifest);
  }

  private async appendPagedIndexesFromStaging(pageSize = DEFAULT_PAGE_SIZE): Promise<void> {
    await fsp.mkdir(this.indexDirectory, { recursive: true });
    const manifest = (await readPagedManifest(this.indexDirectory)) ?? {
      version: 1,
      pageSize,
      createdAt: Date.now(),
      compression: { codec: 'none' },
      lookups: [],
    };

    // 若未显式传入，则沿用 manifest.pageSize，避免与初建不一致
    if (pageSize === DEFAULT_PAGE_SIZE && manifest.pageSize) {
      // eslint-disable-next-line no-param-reassign
      pageSize = manifest.pageSize;
    }

    const lookupMap = new Map<IndexOrder, { order: IndexOrder; pages: PageMeta[] }>(
      manifest.lookups.map((l) => [l.order, { order: l.order, pages: l.pages }]),
    );

    const orders: IndexOrder[] = ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
    for (const order of orders) {
      const staged = this.indexes.get(order);
      if (staged.length === 0) continue;

      const filePath = join(this.indexDirectory, pageFileName(order));
      const writer = new PagedIndexWriter(filePath, {
        directory: this.indexDirectory,
        pageSize,
        compression: manifest.compression,
      });
      const getPrimary = primarySelector(order);
      for (const t of staged) {
        writer.push(t, getPrimary(t));
      }
      const newPages = await writer.finalize();

      const existed = lookupMap.get(order) ?? { order, pages: [] };
      existed.pages.push(...newPages);
      lookupMap.set(order, existed);
    }

    const lookups = [...lookupMap.values()];
    const newManifest: PagedIndexManifest = {
      version: 1,
      pageSize,
      createdAt: Date.now(),
      compression: manifest.compression,
      lookups,
      epoch: (manifest.epoch ?? 0) + 1,
    };
    await writePagedManifest(this.indexDirectory, newManifest);
    this.hydratePagedReaders(newManifest);
    this.currentEpoch = newManifest.epoch ?? this.currentEpoch;

    // 清空 staging
    this.indexes.seed([]);
  }

  addFact(fact: FactInput): PersistedFact {
    // 外部未开启批次则自动包裹 BEGIN/COMMIT
    const autoBatch = this.batchDepth === 0;
    if (autoBatch) void this.wal.appendBegin();
    void this.wal.appendAddTriple(fact);
    if (autoBatch) void this.wal.appendCommit();
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
      this.indexes.add(triple);
      this.dirty = true;
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
      this.indexes.add(triple);
      this.dirty = true;
    } else {
      // 已存在于主文件：为了查询可见性，仍将其加入暂存索引并标记脏，直到下一次 flush 合并分页
      this.indexes.add(triple);
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
    return this.resolveRecords(this.triples.list());
  }

  getDictionarySize(): number {
    return this.dictionary.size;
  }

  getNodeIdByValue(value: string): number | undefined {
    return this.dictionary.getId(value);
  }

  getNodeValueById(id: number): string | undefined {
    return this.dictionary.getValue(id);
  }

  deleteFact(fact: FactInput): void {
    const autoBatch = this.batchDepth === 0;
    if (autoBatch) void this.wal.appendBegin();
    void this.wal.appendDeleteTriple(fact);
    if (autoBatch) void this.wal.appendCommit();
    this.deleteFactDirect(fact);
  }

  private deleteFactDirect(fact: FactInput): void {
    const subjectId = this.dictionary.getOrCreateId(fact.subject);
    const predicateId = this.dictionary.getOrCreateId(fact.predicate);
    const objectId = this.dictionary.getOrCreateId(fact.object);
    this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
    this.dirty = true;
  }

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    const autoBatch = this.batchDepth === 0;
    if (autoBatch) void this.wal.appendBegin();
    void this.wal.appendSetNodeProps(nodeId, properties);
    if (autoBatch) void this.wal.appendCommit();
    this.properties.setNodeProperties(nodeId, properties);
    this.dirty = true;
  }

  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    const autoBatch = this.batchDepth === 0;
    if (autoBatch) void this.wal.appendBegin();
    void this.wal.appendSetEdgeProps(key, properties);
    if (autoBatch) void this.wal.appendCommit();
    this.properties.setEdgeProperties(key, properties);
    this.dirty = true;
  }

  // 事务批次（可选）：外部可将多条写入合并为一个 WAL 批次
  beginBatch(): void {
    if (this.batchDepth === 0) void this.wal.appendBegin();
    this.batchDepth += 1;
  }

  commitBatch(): void {
    if (this.batchDepth > 0) this.batchDepth -= 1;
    if (this.batchDepth === 0) void this.wal.appendCommit();
  }

  abortBatch(): void {
    // 放弃当前批次及所有嵌套
    this.batchDepth = 0;
    void this.wal.appendAbort();
  }

  private setNodePropertiesDirect(nodeId: number, properties: Record<string, unknown>): void {
    this.properties.setNodeProperties(nodeId, properties);
    this.dirty = true;
  }

  private setEdgePropertiesDirect(key: TripleKey, properties: Record<string, unknown>): void {
    this.properties.setEdgeProperties(key, properties);
    this.dirty = true;
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | undefined {
    return this.properties.getNodeProperties(nodeId);
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined {
    return this.properties.getEdgeProperties(key);
  }

  query(criteria: Partial<EncodedTriple>): EncodedTriple[] {
    const now = Date.now();
    if (this.pinnedEpochStack.length === 0 && now - this.lastManifestCheck > 1000) {
      void this.refreshReadersIfEpochAdvanced();
      this.lastManifestCheck = now;
    }
    const order = getBestIndexKey(criteria);
    const reader = this.pagedReaders.get(order);
    const primaryValue = criteria[primaryKey(order)];

    if (!this.dirty && reader && primaryValue !== undefined) {
      this.bumpHot(order, primaryValue as number);
      const triples = reader.readSync(primaryValue);
      return triples.filter(
        (t) => matchCriteria(t, criteria) && !this.tombstones.has(encodeTripleKey(t)),
      );
    }

    return this.indexes.query(criteria).filter((t) => !this.tombstones.has(encodeTripleKey(t)));
  }

  resolveRecords(triples: EncodedTriple[]): FactRecord[] {
    const seen = new Set<string>();
    const results: FactRecord[] = [];
    for (const t of triples) {
      if (this.tombstones.has(encodeTripleKey(t))) continue;
      const key = encodeTripleKey(t);
      if (seen.has(key)) continue;
      seen.add(key);
      results.push(this.toFactRecord(t));
    }
    return results;
  }

  private toFactRecord(triple: EncodedTriple): FactRecord {
    const tripleKey: TripleKey = {
      subjectId: triple.subjectId,
      predicateId: triple.predicateId,
      objectId: triple.objectId,
    };

    return {
      subject: this.dictionary.getValue(triple.subjectId) ?? '',
      predicate: this.dictionary.getValue(triple.predicateId) ?? '',
      object: this.dictionary.getValue(triple.objectId) ?? '',
      subjectId: triple.subjectId,
      predicateId: triple.predicateId,
      objectId: triple.objectId,
      subjectProperties: this.properties.getNodeProperties(triple.subjectId),
      objectProperties: this.properties.getNodeProperties(triple.objectId),
      edgeProperties: this.properties.getEdgeProperties(tripleKey),
    };
  }

  async flush(): Promise<void> {
    if (!this.dirty) {
      return;
    }

    const sections = {
      dictionary: this.dictionary.serialize(),
      triples: this.triples.serialize(),
      indexes: this.indexes.serialize(),
      properties: this.properties.serialize(),
    };
    // 崩溃注入：主文件写入前
    triggerCrash('before-main-write');
    await writeStorageFile(this.path, sections);
    this.dirty = false;
    // 增量刷新分页索引（仅写入新增的 staging）
    triggerCrash('before-page-append');
    await this.appendPagedIndexesFromStaging();
    // 将 tombstones 写入 manifest 以便重启恢复
    const manifest = (await readPagedManifest(this.indexDirectory)) ?? {
      version: 1,
      pageSize: DEFAULT_PAGE_SIZE,
      createdAt: Date.now(),
      compression: { codec: 'none' },
      lookups: [],
    };
    manifest.tombstones = [...this.tombstones]
      .map((k) => decodeTripleKey(k))
      .map((ids) => [ids.subjectId, ids.predicateId, ids.objectId] as [number, number, number]);
    triggerCrash('before-manifest-write');
    await writePagedManifest(this.indexDirectory, manifest);
    // 持久化热度计数（带半衰衰减）
    if (this.hotness) {
      const now = Date.now();
      const halfLifeMs = 10 * 60 * 1000; // 10 分钟半衰期
      const decay = (elapsed: number) => {
        const k = Math.pow(0.5, elapsed / halfLifeMs);
        return k;
      };
      const elapsed = now - (this.hotness.updatedAt ?? now);
      if (elapsed > 0) {
        (Object.keys(this.hotness.counts) as Array<keyof typeof this.hotness.counts>).forEach((order) => {
          const bucket = this.hotness!.counts[order] ?? {};
          const factor = decay(elapsed);
          for (const key of Object.keys(bucket)) {
            bucket[key] = Math.floor(bucket[key] * factor);
            if (bucket[key] <= 0) delete bucket[key];
          }
          this.hotness!.counts[order] = bucket;
        });
      }
      await writeHotness(this.indexDirectory, this.hotness);
    }
    triggerCrash('before-wal-reset');
    await this.wal.reset();
  }

  private async refreshReadersIfEpochAdvanced(): Promise<void> {
    try {
      const manifest = await readPagedManifest(this.indexDirectory);
      if (!manifest) return;
      const epoch = manifest.epoch ?? 0;
      if (epoch > this.currentEpoch) {
        this.hydratePagedReaders(manifest);
        this.currentEpoch = epoch;
      }
    } catch {
      // ignore
    }
  }

  // 读一致性：在查询链路中临时固定 epoch，避免中途重载 readers
  pushPinnedEpoch(epoch: number): void {
    this.pinnedEpochStack.push(epoch);
  }
  popPinnedEpoch(): void {
    this.pinnedEpochStack.pop();
  }
  getCurrentEpoch(): number {
    return this.currentEpoch;
  }

  async close(): Promise<void> {
    // 释放写锁
    if (this.lock) {
      await this.lock.release();
      this.lock = undefined;
    }
    if (this.readerRegistered) {
      try {
        await removeReader(this.indexDirectory, process.pid);
      } catch {}
      this.readerRegistered = false;
    }
  }

  private bumpHot(order: IndexOrder, primary: number): void {
    if (!this.hotness) return;
    const bucket = (this.hotness.counts as Record<IndexOrder, Record<string, number>>)[order] ?? {};
    const key = String(primary);
    bucket[key] = (bucket[key] ?? 0) + 1;
    (this.hotness.counts as Record<IndexOrder, Record<string, number>>)[order] = bucket;
  }
}

function primaryKey(order: IndexOrder): keyof EncodedTriple {
  return order === 'SPO' ? 'subjectId' : order === 'POS' ? 'predicateId' : 'objectId';
}

function primarySelector(order: IndexOrder): (t: EncodedTriple) => number {
  if (order === 'SPO') return (t) => t.subjectId;
  if (order === 'POS') return (t) => t.predicateId;
  return (t) => t.objectId;
}

function matchCriteria(t: EncodedTriple, criteria: Partial<EncodedTriple>): boolean {
  if (criteria.subjectId !== undefined && t.subjectId !== criteria.subjectId) return false;
  if (criteria.predicateId !== undefined && t.predicateId !== criteria.predicateId) return false;
  if (criteria.objectId !== undefined && t.objectId !== criteria.objectId) return false;
  return true;
}

function encodeTripleKey({ subjectId, predicateId, objectId }: EncodedTriple): string {
  return `${subjectId}:${predicateId}:${objectId}`;
}

function decodeTripleKey(key: string): {
  subjectId: number;
  predicateId: number;
  objectId: number;
} {
  const [s, p, o] = key.split(':').map((x) => Number(x));
  return { subjectId: s, predicateId: p, objectId: o };
}

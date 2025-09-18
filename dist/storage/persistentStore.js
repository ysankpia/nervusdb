import { promises as fsp } from 'node:fs';
import { join } from 'node:path';
import { initializeIfMissing, readStorageFile, writeStorageFile } from './fileHeader';
import { StringDictionary } from './dictionary';
import { PropertyStore } from './propertyStore';
import { TripleIndexes, getBestIndexKey } from './tripleIndexes';
import { TripleStore } from './tripleStore';
import { PagedIndexReader, PagedIndexWriter, pageFileName, readPagedManifest, writePagedManifest, DEFAULT_PAGE_SIZE, } from './pagedIndex';
import { WalReplayer, WalWriter } from './wal';
import { readHotness, writeHotness } from './hotness';
import { acquireLock } from '../utils/lock';
import { triggerCrash } from '../utils/fault';
export class PersistentStore {
    path;
    dictionary;
    triples;
    properties;
    indexes;
    indexDirectory;
    constructor(path, dictionary, triples, properties, indexes, indexDirectory) {
        this.path = path;
        this.dictionary = dictionary;
        this.triples = triples;
        this.properties = properties;
        this.indexes = indexes;
        this.indexDirectory = indexDirectory;
    }
    dirty = false;
    wal;
    tombstones = new Set();
    hotness = null;
    lock;
    batchDepth = 0;
    static async open(path, options = {}) {
        await initializeIfMissing(path);
        const sections = await readStorageFile(path);
        const dictionary = StringDictionary.deserialize(sections.dictionary);
        const triples = TripleStore.deserialize(sections.triples);
        const propertyStore = PropertyStore.deserialize(sections.properties);
        const indexes = TripleIndexes.deserialize(sections.indexes);
        // 初次打开且无 manifest 时，将以全量方式重建分页索引，无需在内存中保有全部索引
        const indexDirectory = options.indexDirectory ?? `${path}.pages`;
        const store = new PersistentStore(path, dictionary, triples, propertyStore, indexes, indexDirectory);
        if (options.enableLock) {
            store.lock = await acquireLock(path);
        }
        // WAL 重放（将未持久化的增量恢复到内存与 staging）
        store.wal = await WalWriter.open(path);
        const replay = await new WalReplayer(path).replay();
        for (const f of replay.addFacts)
            store.addFactDirect(f);
        for (const f of replay.deleteFacts)
            store.deleteFactDirect(f);
        for (const n of replay.nodeProps)
            store.setNodePropertiesDirect(n.nodeId, n.value);
        for (const e of replay.edgeProps)
            store.setEdgePropertiesDirect(e.ids, e.value);
        // 截断 WAL 尾部不完整记录，确保下次打开幂等
        if (replay.safeOffset > 0) {
            await store.wal.truncateTo(replay.safeOffset);
        }
        const manifest = await readPagedManifest(indexDirectory);
        const shouldRebuild = options.rebuildIndexes === true ||
            !manifest ||
            manifest.pageSize !== (options.pageSize ?? DEFAULT_PAGE_SIZE);
        if (shouldRebuild) {
            await store.buildPagedIndexes(options.pageSize, options.compression);
        }
        else {
            store.hydratePagedReaders(manifest);
        }
        // 加载热度计数
        try {
            store.hotness = await readHotness(indexDirectory);
        }
        catch {
            store.hotness = { version: 1, updatedAt: Date.now(), counts: { SPO: {}, SOP: {}, POS: {}, PSO: {}, OSP: {}, OPS: {} } };
        }
        return store;
    }
    pagedReaders = new Map();
    hydratePagedReaders(manifest) {
        for (const lookup of manifest.lookups) {
            this.pagedReaders.set(lookup.order, new PagedIndexReader({ directory: this.indexDirectory, compression: manifest.compression }, lookup));
        }
        if (manifest.tombstones && manifest.tombstones.length > 0) {
            manifest.tombstones.forEach(([subjectId, predicateId, objectId]) => {
                this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
            });
        }
    }
    async buildPagedIndexes(pageSize = DEFAULT_PAGE_SIZE, compression = { codec: 'none' }) {
        await fsp.mkdir(this.indexDirectory, { recursive: true });
        const orders = ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
        const lookups = [];
        for (const order of orders) {
            const filePath = join(this.indexDirectory, pageFileName(order));
            try {
                await fsp.unlink(filePath);
            }
            catch {
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
            this.pagedReaders.set(order, new PagedIndexReader({ directory: this.indexDirectory, compression }, { order, pages }));
            lookups.push({ order, pages });
        }
        const manifest = {
            version: 1,
            pageSize,
            createdAt: Date.now(),
            compression,
            lookups,
        };
        await writePagedManifest(this.indexDirectory, manifest);
    }
    async appendPagedIndexesFromStaging(pageSize = DEFAULT_PAGE_SIZE) {
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
        const lookupMap = new Map(manifest.lookups.map((l) => [l.order, { order: l.order, pages: l.pages }]));
        const orders = ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
        for (const order of orders) {
            const staged = this.indexes.get(order);
            if (staged.length === 0)
                continue;
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
        const newManifest = {
            version: 1,
            pageSize,
            createdAt: Date.now(),
            compression: manifest.compression,
            lookups,
            epoch: (manifest.epoch ?? 0) + 1,
        };
        await writePagedManifest(this.indexDirectory, newManifest);
        this.hydratePagedReaders(newManifest);
        // 清空 staging
        this.indexes.seed([]);
    }
    addFact(fact) {
        // 外部未开启批次则自动包裹 BEGIN/COMMIT
        const autoBatch = this.batchDepth === 0;
        if (autoBatch)
            void this.wal.appendBegin();
        void this.wal.appendAddTriple(fact);
        if (autoBatch)
            void this.wal.appendCommit();
        const subjectId = this.dictionary.getOrCreateId(fact.subject);
        const predicateId = this.dictionary.getOrCreateId(fact.predicate);
        const objectId = this.dictionary.getOrCreateId(fact.object);
        const triple = {
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
    addFactDirect(fact) {
        const subjectId = this.dictionary.getOrCreateId(fact.subject);
        const predicateId = this.dictionary.getOrCreateId(fact.predicate);
        const objectId = this.dictionary.getOrCreateId(fact.object);
        const triple = {
            subjectId,
            predicateId,
            objectId,
        };
        if (!this.triples.has(triple)) {
            this.triples.add(triple);
            this.indexes.add(triple);
            this.dirty = true;
        }
        else {
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
    listFacts() {
        return this.resolveRecords(this.triples.list());
    }
    getDictionarySize() {
        return this.dictionary.size;
    }
    getNodeIdByValue(value) {
        return this.dictionary.getId(value);
    }
    getNodeValueById(id) {
        return this.dictionary.getValue(id);
    }
    deleteFact(fact) {
        const autoBatch = this.batchDepth === 0;
        if (autoBatch)
            void this.wal.appendBegin();
        void this.wal.appendDeleteTriple(fact);
        if (autoBatch)
            void this.wal.appendCommit();
        this.deleteFactDirect(fact);
    }
    deleteFactDirect(fact) {
        const subjectId = this.dictionary.getOrCreateId(fact.subject);
        const predicateId = this.dictionary.getOrCreateId(fact.predicate);
        const objectId = this.dictionary.getOrCreateId(fact.object);
        this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
        this.dirty = true;
    }
    setNodeProperties(nodeId, properties) {
        const autoBatch = this.batchDepth === 0;
        if (autoBatch)
            void this.wal.appendBegin();
        void this.wal.appendSetNodeProps(nodeId, properties);
        if (autoBatch)
            void this.wal.appendCommit();
        this.properties.setNodeProperties(nodeId, properties);
        this.dirty = true;
    }
    setEdgeProperties(key, properties) {
        const autoBatch = this.batchDepth === 0;
        if (autoBatch)
            void this.wal.appendBegin();
        void this.wal.appendSetEdgeProps(key, properties);
        if (autoBatch)
            void this.wal.appendCommit();
        this.properties.setEdgeProperties(key, properties);
        this.dirty = true;
    }
    // 事务批次（可选）：外部可将多条写入合并为一个 WAL 批次
    beginBatch() {
        if (this.batchDepth === 0)
            void this.wal.appendBegin();
        this.batchDepth += 1;
    }
    commitBatch() {
        if (this.batchDepth > 0)
            this.batchDepth -= 1;
        if (this.batchDepth === 0)
            void this.wal.appendCommit();
    }
    abortBatch() {
        // 放弃当前批次及所有嵌套
        this.batchDepth = 0;
        void this.wal.appendAbort();
    }
    setNodePropertiesDirect(nodeId, properties) {
        this.properties.setNodeProperties(nodeId, properties);
        this.dirty = true;
    }
    setEdgePropertiesDirect(key, properties) {
        this.properties.setEdgeProperties(key, properties);
        this.dirty = true;
    }
    getNodeProperties(nodeId) {
        return this.properties.getNodeProperties(nodeId);
    }
    getEdgeProperties(key) {
        return this.properties.getEdgeProperties(key);
    }
    query(criteria) {
        const order = getBestIndexKey(criteria);
        const reader = this.pagedReaders.get(order);
        const primaryValue = criteria[primaryKey(order)];
        if (!this.dirty && reader && primaryValue !== undefined) {
            this.bumpHot(order, primaryValue);
            const triples = reader.readSync(primaryValue);
            return triples.filter((t) => matchCriteria(t, criteria) && !this.tombstones.has(encodeTripleKey(t)));
        }
        return this.indexes.query(criteria).filter((t) => !this.tombstones.has(encodeTripleKey(t)));
    }
    resolveRecords(triples) {
        const seen = new Set();
        const results = [];
        for (const t of triples) {
            if (this.tombstones.has(encodeTripleKey(t)))
                continue;
            const key = encodeTripleKey(t);
            if (seen.has(key))
                continue;
            seen.add(key);
            results.push(this.toFactRecord(t));
        }
        return results;
    }
    toFactRecord(triple) {
        const tripleKey = {
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
    async flush() {
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
            .map((ids) => [ids.subjectId, ids.predicateId, ids.objectId]);
        triggerCrash('before-manifest-write');
        await writePagedManifest(this.indexDirectory, manifest);
        // 持久化热度计数
        if (this.hotness) {
            await writeHotness(this.indexDirectory, this.hotness);
        }
        triggerCrash('before-wal-reset');
        await this.wal.reset();
    }
    async close() {
        // 释放写锁
        if (this.lock) {
            await this.lock.release();
            this.lock = undefined;
        }
    }
    bumpHot(order, primary) {
        if (!this.hotness)
            return;
        const bucket = this.hotness.counts[order] ?? {};
        const key = String(primary);
        bucket[key] = (bucket[key] ?? 0) + 1;
        this.hotness.counts[order] = bucket;
    }
}
function primaryKey(order) {
    return order === 'SPO' ? 'subjectId' : order === 'POS' ? 'predicateId' : 'objectId';
}
function primarySelector(order) {
    if (order === 'SPO')
        return (t) => t.subjectId;
    if (order === 'POS')
        return (t) => t.predicateId;
    return (t) => t.objectId;
}
function matchCriteria(t, criteria) {
    if (criteria.subjectId !== undefined && t.subjectId !== criteria.subjectId)
        return false;
    if (criteria.predicateId !== undefined && t.predicateId !== criteria.predicateId)
        return false;
    if (criteria.objectId !== undefined && t.objectId !== criteria.objectId)
        return false;
    return true;
}
function encodeTripleKey({ subjectId, predicateId, objectId }) {
    return `${subjectId}:${predicateId}:${objectId}`;
}
function decodeTripleKey(key) {
    const [s, p, o] = key.split(':').map((x) => Number(x));
    return { subjectId: s, predicateId: p, objectId: o };
}
//# sourceMappingURL=persistentStore.js.map